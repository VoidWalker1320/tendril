use pumpkin_data::translation;
use pumpkin_protocol::bedrock::client::PackIdVersion;
use pumpkin_util::text::TextComponent;

#[allow(clippy::wildcard_imports)]
use super::*;

/// Builds the name a Bedrock player is known by on the server.
///
/// Bedrock gamertags may contain spaces, which cannot be typed as a command argument,
/// and may collide with a Java account name on a cross-play server. `prefix` is
/// prepended as-is (empty means no prefix) and, when `replace_spaces` is set, spaces
/// become underscores.
fn build_username(display_name: &str, prefix: &str, replace_spaces: bool) -> String {
    let mut name = String::with_capacity(prefix.len() + display_name.len());
    name.push_str(prefix);
    if replace_spaces {
        name.extend(display_name.chars().map(|c| if c == ' ' { '_' } else { c }));
    } else {
        name.push_str(display_name);
    }
    name
}

/// The player's UUID: Floodgate's `UUID(0, xuid)` when asked for and an XUID is present,
/// otherwise the identity UUID from the login token.
fn player_uuid(
    claims: &pumpkin_auth::jwt::PlayerClaims,
    floodgate_uuids: bool,
) -> Result<Uuid, LoginError> {
    if floodgate_uuids && let Ok(xuid) = claims.xuid.parse::<u64>() {
        return Ok(Uuid::from_u64_pair(0, xuid));
    }
    Uuid::parse_str(&claims.uuid).map_err(|_| LoginError::InvalidUuid)
}

/// Checks the login token. Returns the player's claims and whether their XUID is proven: only a
/// fully authenticated (Xbox) login proves it; any other login can claim any XUID.
async fn authenticate(
    server: &Arc<Server>,
    auth_payload: &AuthPayload,
) -> Result<(pumpkin_auth::jwt::PlayerClaims, bool), LoginError> {
    Ok(if server.advanced_config.networking.bedrock.online_mode {
        match auth_payload.authentication_type {
            AuthenticationType::Full => (
                verify_oidc_token_path(server, &auth_payload.token, false).await?,
                true,
            ),
            AuthenticationType::SelfSigned => {
                if server
                    .advanced_config
                    .networking
                    .bedrock
                    .authentication
                    .enabled
                {
                    return Err(LoginError::SelfSignedNotAllowed);
                }

                (
                    verify_oidc_token_path(server, &auth_payload.token, true).await?,
                    false,
                )
            }
            AuthenticationType::Guest => {
                return Err(LoginError::GuestUnimplemented);
            }
        }
    } else {
        (
            pumpkin_auth::jwt::extract_oidc_token_player_claims(&auth_payload.token)?,
            false,
        )
    })
}

impl BedrockClient {
    pub async fn handle_login(
        self: &Arc<Self>,
        packet: SLogin,
        server: &Arc<Server>,
    ) -> Result<PacketHandlerResult, LoginError> {
        self.try_handle_login(packet, server).await
    }

    pub async fn try_handle_login(
        self: &Arc<Self>,
        packet: SLogin,
        server: &Arc<Server>,
    ) -> Result<PacketHandlerResult, LoginError> {
        let auth_payload: AuthPayload = serde_json::from_slice(&packet.jwt)?;
        let (player_data, xuid_verified) = authenticate(server, &auth_payload).await?;

        let raw_token_str = std::str::from_utf8(&packet.raw_token).map_err(|_| {
            LoginError::InvalidTokenFormat(serde_json::Error::custom(
                "raw_token is not valid UTF-8",
            ))
        })?; // You'll need to add a string conversion error to LoginError, or handle it cleanly.

        let mut parts = raw_token_str.split('.');
        let _header = parts.next().ok_or(AuthError::InvalidTokenFormat)?;
        let payload_b64 = parts.next().ok_or(AuthError::InvalidTokenFormat)?;

        let payload_bytes = pumpkin_auth::jwt::decode_b64_url_nopad(payload_b64)
            .map_err(|_| LoginError::DecodeExtraError)?;
        let client_data: ClientData = serde_json::from_slice(&payload_bytes)?;

        let bedrock_config = &server.advanced_config.networking.bedrock;
        let name = build_username(
            &player_data.display_name,
            &bedrock_config.username_prefix,
            bedrock_config.replace_username_spaces,
        );

        let mut profile = GameProfile {
            id: player_uuid(
                &player_data,
                bedrock_config.floodgate_uuids && xuid_verified,
            )?,
            name,
            properties: ArcSwap::new(Arc::new(Vec::new())),
            profile_actions: None,
        };
        // A linked Bedrock account plays as its Java account, so duplicate checks see that one.
        if xuid_verified
            && let Some(links) = server.bedrock_account_links.get()
            && let Some((id, name)) = links(&player_data.xuid)
        {
            profile.id = id;
            profile.name = name;
        }

        let login_public_key = pumpkin_auth::jwt::extract_cpk_from_token(&auth_payload.token)
            .map_err(LoginError::ChainValidationFailed)?;
        if self
            .nethernet_public_key()
            .is_some_and(|public_key| public_key != &login_public_key)
        {
            return Err(LoginError::ChainValidationFailed(
                AuthError::PublicKeyBuild(
                    "NetherNet and Bedrock login identities do not match".into(),
                ),
            ));
        }

        if !self.can_complete_login(&profile, server).await {
            return Ok(PacketHandlerResult::Stop);
        }

        self.enqueue_client_packet(&CPlayStatus::LoginSuccess).await;
        let br_config = &server.advanced_config.resource_pack.bedrock;

        let mut entries = Vec::new();
        if br_config.enabled {
            for pack in &br_config.packs {
                entries.push(PackInfoData {
                    pack_id_version: PackIdVersion::new(pack.uuid, pack.version.clone()),
                    pack_size: pack.size,
                    cdn_url: pack.download_url.clone(),
                    content_key: pack.content_key.clone(),
                    subpack_name: pack.sub_pack_name.clone(),
                    content_identity: pack.content_id.clone(),
                    has_scripts: pack.has_scripts,
                    is_addon_pack: pack.addon_pack,
                    is_ray_tracing_capable: pack.rtx_enabled,
                });
            }
        }

        let packs_info = CResourcePacksInfo {
            resource_pack_required: br_config.force,
            has_addon_packs: false,
            has_scripts: false,
            force_disable_vibrant_visuals: false,
            world_template_id_and_version: PackIdVersion::new(uuid::Uuid::nil(), String::new()),
            resource_packs: entries,
        };
        self.enqueue_client_packet(&packs_info).await;

        let new_config = PlayerConfig {
            locale: client_data.language_code.clone(),
            ..Default::default()
        };

        self.client_data
            .store(std::sync::Arc::new(Some(std::sync::Arc::new(client_data))));

        Ok(PacketHandlerResult::ReadyToPlay(profile, new_config))
    }

    async fn can_complete_login(
        self: &Arc<Self>,
        profile: &GameProfile,
        server: &Arc<Server>,
    ) -> bool {
        server
            .data
            .user_cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .upsert(profile.id, profile.name.clone());

        let max_players = server.advanced_config.networking.bedrock.max_players;
        if max_players > 0 && server.get_player_count() >= max_players as usize {
            self.kick(
                DisconnectReason::ServerFull,
                TextComponent::translate_cross(
                    translation::java::MULTIPLAYER_DISCONNECT_SERVER_FULL,
                    translation::bedrock::DISCONNECTIONSCREEN_SERVERFULL,
                    [],
                )
                .get_text(),
            )
            .await;
            return false;
        }

        if let Some(reason) = crate::net::can_not_join(profile, &self.address, server).await {
            self.kick(DisconnectReason::NotAllowed, reason.get_text())
                .await;
            return false;
        }

        if server.get_player_by_uuid(profile.id).is_some()
            || server.get_player_by_name(&profile.name).is_some()
        {
            self.kick(
                DisconnectReason::LoggedInOtherLocation,
                TextComponent::translate_cross(
                    translation::java::MULTIPLAYER_DISCONNECT_DUPLICATE_LOGIN,
                    translation::bedrock::DISCONNECTIONSCREEN_LOGGEDINOTHERLOCATION,
                    [],
                )
                .get_text(),
            )
            .await;
            return false;
        }

        let mut pre_login_event =
            crate::plugin::api::events::player::async_player_pre_login::AsyncPlayerPreLoginEvent {
                player_name: profile.name.clone(),
                player_uuid: profile.id,
                ip_address: self.address,
                kick_message: TextComponent::text("Disconnected"),
                cancelled: false,
            };
        server
            .plugin_manager
            .fire(server, &mut pre_login_event)
            .await;
        if pre_login_event.cancelled {
            self.kick(
                DisconnectReason::Kicked,
                pre_login_event.kick_message.get_text(),
            )
            .await;
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::{build_username, player_uuid};
    use pumpkin_auth::jwt::PlayerClaims;

    const TOKEN_UUID: &str = "5f1c6a3e-9d2b-4b0a-8c7e-2a1f3b4c5d6e";

    fn claims(xuid: &str) -> PlayerClaims {
        PlayerClaims {
            display_name: "Steve".into(),
            uuid: TOKEN_UUID.into(),
            xuid: xuid.into(),
        }
    }

    #[test]
    fn floodgate_uuid_is_derived_from_the_xuid() {
        let uuid = player_uuid(&claims("2535419381938497"), true).unwrap();
        assert_eq!(uuid.to_string(), "00000000-0000-0000-0009-01f3539df941");
    }

    #[test]
    fn token_uuid_is_kept_without_floodgate_uuids_or_an_xuid() {
        assert_eq!(
            player_uuid(&claims("2535419381938497"), false)
                .unwrap()
                .to_string(),
            TOKEN_UUID
        );
        assert_eq!(
            player_uuid(&claims(""), true).unwrap().to_string(),
            TOKEN_UUID
        );
    }

    #[test]
    fn spaces_become_underscores_by_default() {
        assert_eq!(build_username("Some Player", "", true), "Some_Player");
    }

    #[test]
    fn spaces_are_kept_when_replacement_is_disabled() {
        assert_eq!(build_username("Some Player", "", false), "Some Player");
    }

    #[test]
    fn prefix_is_prepended() {
        assert_eq!(build_username("Some Player", ".", true), ".Some_Player");
    }

    #[test]
    fn empty_prefix_leaves_the_name_unchanged() {
        assert_eq!(build_username("Steve", "", true), "Steve");
    }
}
