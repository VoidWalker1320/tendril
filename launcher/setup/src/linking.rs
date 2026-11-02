//! Bedrock-to-Java account links (section 8), kept in `data/linked-accounts.json`.
//!
//! A Java player asks for a code naming a Bedrock player's in-game name; that Bedrock player
//! confirms with the Java name and the code within five minutes. From then on the Bedrock player
//! logs in with the Java account's UUID and name, like Floodgate's local linking.

use std::hash::{BuildHasher, RandomState};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "linked-accounts.json";
pub const CODE_LIFETIME: Duration = Duration::from_secs(300);
/// Wrong codes before a request is dropped. A code is bound to one in-game name, so guessing a
/// six-digit code in three tries is the only way around it.
const ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub xuid: String,
    pub bedrock_name: String,
    pub java_uuid: String,
    pub java_name: String,
}

#[derive(Debug)]
struct Request {
    java_uuid: String,
    java_name: String,
    bedrock_name: String,
    code: String,
    expires: Instant,
    attempts_left: u8,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("{0} has not asked to link with you; they need to run /linkaccount <your name> first")]
    NoRequest(String),
    #[error("wrong code")]
    WrongCode,
    #[error("too many wrong codes; ask for a new one")]
    TooManyAttempts,
    #[error("the code has expired; ask for a new one")]
    Expired,
    #[error("this Bedrock account is already linked to {0}; use /unlinkaccount first")]
    BedrockAlreadyLinked(String),
    #[error("your account is already linked to {0}; use /unlinkaccount first")]
    JavaAlreadyLinked(String),
    #[error("cannot save the link: {0}")]
    Save(String),
}

pub struct LinkStore {
    path: PathBuf,
    links: Vec<Link>,
    requests: Vec<Request>,
}

impl LinkStore {
    /// Reads the links from `path`; a missing file means no links.
    ///
    /// # Errors
    /// Fails if the file exists but cannot be read or parsed.
    pub fn load(path: PathBuf) -> io::Result<Self> {
        let links = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path,
            links,
            requests: Vec::new(),
        })
    }

    /// The Java identity (UUID, name) a Bedrock player with this XUID logs in as.
    #[must_use]
    pub fn identity_for(&self, xuid: &str) -> Option<(&str, &str)> {
        self.links
            .iter()
            .find(|l| l.xuid == xuid)
            .map(|l| (l.java_uuid.as_str(), l.java_name.as_str()))
    }

    /// Records a Java player's request to link with `bedrock_name` and returns the code.
    ///
    /// # Errors
    /// Fails if the Java account is already linked.
    pub fn request(
        &mut self,
        java_uuid: &str,
        java_name: &str,
        bedrock_name: &str,
        now: Instant,
    ) -> Result<String, LinkError> {
        if let Some(link) = self.links.iter().find(|l| l.java_uuid == java_uuid) {
            return Err(LinkError::JavaAlreadyLinked(link.bedrock_name.clone()));
        }
        self.requests
            .retain(|r| r.java_uuid != java_uuid && r.expires > now);
        let code = new_code();
        self.requests.push(Request {
            java_uuid: java_uuid.to_string(),
            java_name: java_name.to_string(),
            bedrock_name: bedrock_name.to_string(),
            code: code.clone(),
            expires: now + CODE_LIFETIME,
            attempts_left: ATTEMPTS,
        });
        Ok(code)
    }

    /// A Bedrock player confirms a request from `java_name` with its code; saves the link.
    ///
    /// # Errors
    /// Fails without a matching request, with a wrong or expired code, if either account is
    /// already linked, or if the file cannot be written.
    pub fn confirm(
        &mut self,
        xuid: &str,
        bedrock_name: &str,
        java_name: &str,
        code: &str,
        now: Instant,
    ) -> Result<Link, LinkError> {
        if let Some((_, linked)) = self.identity_for(xuid) {
            return Err(LinkError::BedrockAlreadyLinked(linked.to_string()));
        }
        let index = self
            .requests
            .iter()
            .position(|r| {
                r.java_name.eq_ignore_ascii_case(java_name)
                    && r.bedrock_name.eq_ignore_ascii_case(bedrock_name)
            })
            .ok_or_else(|| LinkError::NoRequest(java_name.to_string()))?;
        let request = &mut self.requests[index];
        if request.expires <= now {
            self.requests.remove(index);
            return Err(LinkError::Expired);
        }
        if request.code != code {
            request.attempts_left -= 1;
            if request.attempts_left == 0 {
                self.requests.remove(index);
                return Err(LinkError::TooManyAttempts);
            }
            return Err(LinkError::WrongCode);
        }
        let request = self.requests.remove(index);
        let link = Link {
            xuid: xuid.to_string(),
            bedrock_name: bedrock_name.to_string(),
            java_uuid: request.java_uuid,
            java_name: request.java_name,
        };
        self.links.push(link.clone());
        if let Err(e) = self.save() {
            self.links.pop();
            return Err(LinkError::Save(e.to_string()));
        }
        Ok(link)
    }

    /// Removes the link of a Java account (which a linked Bedrock player also plays as).
    ///
    /// # Errors
    /// Fails if the file cannot be written; the link is kept then.
    pub fn unlink(&mut self, java_uuid: &str) -> Result<Option<Link>, LinkError> {
        let Some(index) = self.links.iter().position(|l| l.java_uuid == java_uuid) else {
            return Ok(None);
        };
        let link = self.links.remove(index);
        if let Err(e) = self.save() {
            self.links.insert(index, link);
            return Err(LinkError::Save(e.to_string()));
        }
        Ok(Some(link))
    }

    /// Writes all links through a temporary file, so a crash never leaves half a file.
    fn save(&self) -> io::Result<()> {
        let text = serde_json::to_string_pretty(&self.links).map_err(io::Error::other)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, &self.path)
    }
}

/// A six-digit code from the standard library's randomly keyed hasher.
fn new_code() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:06}", RandomState::new().hash_one(nanos) % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAVA_UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
    const XUID: &str = "2535419381938497";

    fn store() -> (tempfile::TempDir, LinkStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = LinkStore::load(dir.path().join("data").join(FILE_NAME)).unwrap();
        (dir, store)
    }

    #[test]
    fn request_then_confirm_links_and_persists() {
        let (dir, mut s) = store();
        let now = Instant::now();
        let code = s.request(JAVA_UUID, "Notch", ".Some_Player", now).unwrap();
        assert_eq!(code.len(), 6);
        let link = s
            .confirm(XUID, ".some_player", "notch", &code, now)
            .unwrap();
        assert_eq!(link.java_name, "Notch");
        assert_eq!(s.identity_for(XUID), Some((JAVA_UUID, "Notch")));
        let reloaded = LinkStore::load(dir.path().join("data").join(FILE_NAME)).unwrap();
        assert_eq!(reloaded.identity_for(XUID), Some((JAVA_UUID, "Notch")));
    }

    #[test]
    fn only_the_named_bedrock_player_can_confirm() {
        let (_dir, mut s) = store();
        let now = Instant::now();
        let code = s.request(JAVA_UUID, "Notch", ".Some_Player", now).unwrap();
        assert_eq!(
            s.confirm(XUID, ".Someone_Else", "Notch", &code, now),
            Err(LinkError::NoRequest("Notch".into()))
        );
    }

    #[test]
    fn three_wrong_codes_drop_the_request() {
        let (_dir, mut s) = store();
        let now = Instant::now();
        let code = s.request(JAVA_UUID, "Notch", ".P", now).unwrap();
        let wrong = if code == "000000" { "000001" } else { "000000" };
        assert_eq!(
            s.confirm(XUID, ".P", "Notch", wrong, now),
            Err(LinkError::WrongCode)
        );
        assert_eq!(
            s.confirm(XUID, ".P", "Notch", wrong, now),
            Err(LinkError::WrongCode)
        );
        assert_eq!(
            s.confirm(XUID, ".P", "Notch", wrong, now),
            Err(LinkError::TooManyAttempts)
        );
        assert!(matches!(
            s.confirm(XUID, ".P", "Notch", &code, now),
            Err(LinkError::NoRequest(_))
        ));
    }

    #[test]
    fn codes_expire() {
        let (_dir, mut s) = store();
        let now = Instant::now();
        let code = s.request(JAVA_UUID, "Notch", ".P", now).unwrap();
        assert_eq!(
            s.confirm(XUID, ".P", "Notch", &code, now + CODE_LIFETIME),
            Err(LinkError::Expired)
        );
    }

    #[test]
    fn linked_accounts_must_unlink_before_relinking() {
        let (_dir, mut s) = store();
        let now = Instant::now();
        let code = s.request(JAVA_UUID, "Notch", ".P", now).unwrap();
        s.confirm(XUID, ".P", "Notch", &code, now).unwrap();
        assert_eq!(
            s.request(JAVA_UUID, "Notch", ".Q", now),
            Err(LinkError::JavaAlreadyLinked(".P".into()))
        );
        let other = s
            .request("11111111-2222-3333-4444-555555555555", "Jeb", ".P", now)
            .unwrap();
        assert_eq!(
            s.confirm(XUID, ".P", "Jeb", &other, now),
            Err(LinkError::BedrockAlreadyLinked("Notch".into()))
        );
        assert_eq!(
            s.unlink(JAVA_UUID).unwrap().map(|l| l.xuid),
            Some(XUID.to_string())
        );
        assert_eq!(s.identity_for(XUID), None);
        assert_eq!(s.unlink(JAVA_UUID).unwrap(), None);
    }
}
