//! `/linkaccount` and `/unlinkaccount` (section 8), on top of `tendril_setup::linking`.
//!
//! Java: `/linkaccount <bedrock name>` gives a code. Bedrock: `/linkaccount <java name> <code>`
//! links, and from the next join that Bedrock player logs in as the Java account.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use pumpkin_core::command::argument_builder::{ArgumentBuilder, argument, command};
use pumpkin_core::command::argument_types::core::string::StringArgumentType;
use pumpkin_core::command::context::command_context::CommandContext;
use pumpkin_core::command::node::detached::CommandDetachedNode;
use pumpkin_core::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin_core::net::DisconnectReason;
use pumpkin_core::server::Server;
use pumpkin_util::permission::{Permission, PermissionDefault};
use pumpkin_util::text::TextComponent;
use tendril_setup::linking::{FILE_NAME, LinkStore};
use tracing::{info, warn};

const LINK: &str = "tendril:command.linkaccount";
const LINK_DESCRIPTION: &str = "Links your Bedrock and Java accounts.";
const UNLINK: &str = "tendril:command.unlinkaccount";
const UNLINK_DESCRIPTION: &str = "Removes the link between your Bedrock and Java accounts.";

type Store = Arc<Mutex<LinkStore>>;

fn lock(store: &Store) -> MutexGuard<'_, LinkStore> {
    store.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Loads the links from `data_dir`, hooks them into Bedrock logins and adds the commands.
pub fn install(server: &Arc<Server>, data_dir: &Path) {
    let store = match LinkStore::load(data_dir.join(FILE_NAME)) {
        Ok(store) => store,
        Err(err) => {
            warn!("Account linking is off: cannot read {FILE_NAME}: {err}");
            return;
        }
    };
    let store: Store = Arc::new(Mutex::new(store));
    let lookup = Arc::clone(&store);
    let _ = server.bedrock_account_links.set(Box::new(move |xuid| {
        let store = lock(&lookup);
        let (uuid, name) = store.identity_for(xuid)?;
        Some((uuid::Uuid::parse_str(uuid).ok()?, name.to_string()))
    }));

    let registry = &server.permission_manager.registry;
    for (node, description) in [(LINK, LINK_DESCRIPTION), (UNLINK, UNLINK_DESCRIPTION)] {
        let _ = registry.register_permission(Permission::new(
            node,
            description,
            PermissionDefault::Allow,
        ));
    }
    let link: CommandDetachedNode = command("linkaccount", LINK_DESCRIPTION)
        .requires(LINK)
        .then(
            argument("name", StringArgumentType::SingleWord)
                .executes(Request {
                    store: Arc::clone(&store),
                })
                .then(
                    argument("code", StringArgumentType::SingleWord).executes(Confirm {
                        store: Arc::clone(&store),
                    }),
                ),
        )
        .into();
    let unlink: CommandDetachedNode = command("unlinkaccount", UNLINK_DESCRIPTION)
        .requires(UNLINK)
        .executes(Unlink { store })
        .into();
    server.command_dispatcher.rcu(|dispatcher| {
        let mut next = (**dispatcher).clone();
        next.register(link.clone());
        next.register(unlink.clone());
        Arc::new(next)
    });
}

/// The XUID inside a Floodgate-style UUID (`UUID(0, xuid)`).
fn floodgate_xuid(id: uuid::Uuid) -> Option<String> {
    let (high, low) = id.as_u64_pair();
    (high == 0 && low != 0).then(|| low.to_string())
}

/// Sends `message` to the command's sender and returns the command result (1 done, 0 refused).
fn reply(context: &CommandContext, ok: bool, message: String) -> i32 {
    if ok {
        context.source.send_message(TextComponent::text(message));
        1
    } else {
        context.source.send_error(TextComponent::text(message));
        0
    }
}

/// Java side: `/linkaccount <bedrock name>`.
struct Request {
    store: Store,
}

impl CommandExecutor for Request {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let Some(player) = context.source.as_player() else {
            return Ok(reply(
                context,
                false,
                "Only players can link accounts.".into(),
            ));
        };
        let target = StringArgumentType::get(context, "name")?;
        if player.client.bedrock().is_some() {
            return Ok(reply(
                context,
                false,
                format!("On Bedrock, add the code you were given: /linkaccount {target} <code>"),
            ));
        }
        let profile = &player.gameprofile;
        let result = lock(&self.store).request(
            &profile.id.to_string(),
            &profile.name,
            target,
            Instant::now(),
        );
        Ok(match result {
            Ok(code) => reply(
                context,
                true,
                format!(
                    "Code {code}: within 5 minutes, {target} runs /linkaccount {} {code} on Bedrock.",
                    profile.name
                ),
            ),
            Err(err) => reply(context, false, err.to_string()),
        })
    }
}

/// Bedrock side: `/linkaccount <java name> <code>`.
struct Confirm {
    store: Store,
}

impl CommandExecutor for Confirm {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let Some(player) = context.source.as_player() else {
            return Ok(reply(
                context,
                false,
                "Only players can link accounts.".into(),
            ));
        };
        if player.client.bedrock().is_none() {
            return Ok(reply(
                context,
                false,
                "Enter the code on your Bedrock account; on Java, run /linkaccount <bedrock name>."
                    .into(),
            ));
        }
        let Some(xuid) = floodgate_xuid(player.gameprofile.id) else {
            return Ok(reply(
                context,
                false,
                "Linking needs a Bedrock account signed in to Xbox (online mode).".into(),
            ));
        };
        let java_name = StringArgumentType::get(context, "name")?;
        let code = StringArgumentType::get(context, "code")?;
        let result = lock(&self.store).confirm(
            &xuid,
            &player.gameprofile.name,
            java_name,
            code,
            Instant::now(),
        );
        Ok(match result {
            Ok(link) => {
                info!(
                    "Linked Bedrock player {} (XUID {xuid}) to Java account {}",
                    link.bedrock_name, link.java_name
                );
                player.kick(
                    DisconnectReason::Kicked,
                    &TextComponent::text(format!(
                        "Linked to {0}. Join again to play as {0}.",
                        link.java_name
                    )),
                );
                1
            }
            Err(err) => reply(context, false, err.to_string()),
        })
    }
}

/// Either side: `/unlinkaccount`.
struct Unlink {
    store: Store,
}

impl CommandExecutor for Unlink {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let Some(player) = context.source.as_player() else {
            return Ok(reply(
                context,
                false,
                "Only players can unlink accounts.".into(),
            ));
        };
        let result = lock(&self.store).unlink(&player.gameprofile.id.to_string());
        Ok(match result {
            Ok(Some(link)) => {
                info!(
                    "Unlinked Bedrock player {} from Java account {}",
                    link.bedrock_name, link.java_name
                );
                if player.client.bedrock().is_some() {
                    // The Bedrock player is still logged in as the Java account.
                    player.kick(
                        DisconnectReason::Kicked,
                        &TextComponent::text("Unlinked. Join again to play as your Bedrock self."),
                    );
                    return Ok(1);
                }
                reply(
                    context,
                    true,
                    format!("Unlinked from Bedrock player {}.", link.bedrock_name),
                )
            }
            Ok(None) => reply(context, false, "Your account is not linked.".into()),
            Err(err) => reply(context, false, err.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::floodgate_xuid;

    #[test]
    fn xuid_comes_back_out_of_a_floodgate_uuid() {
        let id = uuid::Uuid::parse_str("00000000-0000-0000-0009-01f3539df941").unwrap();
        assert_eq!(floodgate_xuid(id).as_deref(), Some("2535419381938497"));
        let java = uuid::Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap();
        assert_eq!(floodgate_xuid(java), None);
    }
}
