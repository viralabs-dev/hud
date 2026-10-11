//! Prepared provider adapters. No filesystem/env discovery, probes, or execution.
use crate::{request_with_permission_mode, Config, Error, Profile, Request, Session};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::Duration,
};
#[derive(Clone, Debug)]
pub struct AdapterConfig {
    pub contract: Config,
    pub read_tools: Vec<String>,
    pub cwd: PathBuf,
    pub profile: Profile,
    pub timeout: Duration,
    pub follow_folder: bool,
    pub full_permission_mode: String,
    pub contract_version: String,
}
#[derive(Clone, Debug)]
pub struct PreparedRequest {
    pub request: Request,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub contract_version: String,
}
fn folder_valid(folder: &Path) -> bool {
    folder.is_absolute()
        && !folder
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        && folder
            .to_str()
            .is_some_and(|s| !s.chars().any(char::is_control))
}
impl AdapterConfig {
    pub fn prepare(
        &self,
        session: &Session,
        prompt: &str,
        context: &str,
        environment: &BTreeMap<String, String>,
    ) -> Result<PreparedRequest, Error> {
        if !folder_valid(&self.cwd)
            || self.timeout < Duration::from_secs(10)
            || self.timeout > Duration::from_secs(3600)
            || self.contract_version.is_empty()
        {
            return Err(Error::InvalidConfig);
        }
        if self.read_tools.is_empty()
            || self.read_tools.len() > 3
            || self
                .read_tools
                .iter()
                .any(|t| !["Read", "Grep", "Glob"].contains(&t.as_str()))
            || self
                .read_tools
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.read_tools.len()
        {
            return Err(Error::InvalidConfig);
        }
        let mut request = request_with_permission_mode(
            &self.contract,
            self.profile,
            session,
            prompt,
            context,
            environment,
            &self.full_permission_mode,
        )?;
        if self.contract.provider == crate::Provider::Claude && self.profile == Profile::ReadOnly {
            let argv = &mut request.argv;
            let tools = argv
                .iter()
                .position(|a| a == "--tools")
                .ok_or(Error::InvalidConfig)?;
            argv[tools + 1] = self.read_tools.join(",");
            let first = argv
                .iter()
                .position(|a| a == "--allowedTools")
                .ok_or(Error::InvalidConfig)?
                + 1;
            let end = argv
                .iter()
                .enumerate()
                .skip(first)
                .find(|(_, a)| a.starts_with("--"))
                .map_or(argv.len(), |(i, _)| i);
            let allowed = argv[first..end]
                .iter()
                .filter(|a| {
                    self.read_tools
                        .iter()
                        .any(|t| a.starts_with(&format!("{t}(")))
                })
                .cloned()
                .collect::<Vec<_>>();
            argv.splice(first..end, allowed);
        }
        Ok(PreparedRequest {
            request,
            cwd: self.cwd.clone(),
            timeout: self.timeout,
            contract_version: self.contract_version.clone(),
        })
    }
    /// Pure retarget. Caller validates folder identity/permission before applying UI changes.
    pub fn following_folder(&self, target: &Path, home: &Path) -> Result<Self, Error> {
        if !folder_valid(target) || !folder_valid(home) {
            return Err(Error::InvalidConfig);
        }
        let mut next = self.clone();
        if !self.follow_folder {
            return Ok(next);
        }
        next.cwd = target.to_owned();
        let target = target.to_str().ok_or(Error::InvalidConfig)?;
        if !next.contract.read_dirs.is_empty() {
            next.contract.read_dirs[0] = target.into();
        }
        if !next.contract.read_rules.is_empty() {
            next.contract.read_rules[0] = match Path::new(target).strip_prefix(home) {
                Ok(relative) if relative.as_os_str().is_empty() => "~".into(),
                Ok(relative) => format!("~/{}", relative.to_str().ok_or(Error::InvalidConfig)?),
                Err(_) => format!("/{target}"),
            };
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Provider;
    fn config() -> AdapterConfig {
        AdapterConfig {
            read_tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
            contract: Config {
                provider: Provider::Claude,
                launch: vec!["claude".into()],
                model: "fixture".into(),
                read_dirs: vec!["/home/test/old".into(), "/tmp/extra".into()],
                read_rules: vec!["~/old".into(), "//tmp/extra".into()],
                budget_cents: 125,
            },
            cwd: "/home/test/old".into(),
            profile: Profile::Full,
            timeout: Duration::from_secs(900),
            follow_folder: true,
            full_permission_mode: "plan".into(),
            contract_version: "python-fixture-v1".into(),
        }
    }
    #[test]
    fn readonly_claude_subset_preserves_deny_rules_and_rejects_unsafe_tools() {
        let mut c = config();
        c.profile = Profile::ReadOnly;
        c.read_tools = vec!["Read".into(), "Glob".into()];
        let request = c
            .prepare(&Session::default(), "fixture", "context", &BTreeMap::new())
            .unwrap()
            .request;
        let tools = request.argv.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(request.argv[tools + 1], "Read,Glob");
        let allowed = request
            .argv
            .iter()
            .position(|a| a == "--allowedTools")
            .unwrap()
            + 1;
        let end = request
            .argv
            .iter()
            .enumerate()
            .skip(allowed)
            .find(|(_, a)| a.starts_with("--"))
            .unwrap()
            .0;
        assert!(request.argv[allowed..end]
            .iter()
            .all(|a| !a.starts_with("Grep(")));
        assert!(request.argv.iter().any(|a| a == "Grep(~/.ssh/**)"));
        c.read_tools = vec!["Bash".into()];
        assert!(c
            .prepare(&Session::default(), "fixture", "", &BTreeMap::new())
            .is_err());
    }
    #[test]
    fn adapter_preserves_settings_and_fixed_scope_and_rejects_bypass() {
        let c = config();
        let next = c
            .following_folder(Path::new("/home/test/new"), Path::new("/home/test"))
            .unwrap();
        assert_eq!(next.cwd, Path::new("/home/test/new"));
        assert_eq!(next.contract.read_rules, vec!["~/new", "//tmp/extra"]);
        assert_eq!(
            next.contract.read_dirs,
            vec!["/home/test/new", "/tmp/extra"]
        );
        assert_eq!(next.timeout, Duration::from_secs(900));
        let request = next
            .prepare(&Session::default(), "fixture", "context", &BTreeMap::new())
            .unwrap();
        assert!(request.request.argv.iter().any(|arg| arg == "plan"));
        assert_eq!(request.timeout, next.timeout);
        let mut fixed = c.clone();
        fixed.follow_folder = false;
        assert_eq!(
            fixed
                .following_folder(Path::new("/tmp/new"), Path::new("/home/test"))
                .unwrap()
                .cwd,
            c.cwd
        );
        fixed.full_permission_mode = "bypassPermissions".into();
        assert_eq!(
            fixed
                .prepare(&Session::default(), "fixture", "", &BTreeMap::new())
                .unwrap_err(),
            Error::InvalidConfig
        );
    }
}
