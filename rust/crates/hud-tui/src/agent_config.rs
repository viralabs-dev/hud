//! Explicit private settings: no PATH/provider probes, credentials or implicit personal import.
use hud_agent_protocol::{adapter::AdapterConfig, Config, Profile, Provider, Session};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    time::Duration,
};
fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn text<'a>(v: &'a Value, key: &str, default: &'a str) -> io::Result<&'a str> {
    v.get(key).map_or(Ok(default), |x| {
        x.as_str().ok_or_else(|| bad("setting must be string"))
    })
}
fn directory(raw: &str, base: &Path, home: &Path) -> io::Result<PathBuf> {
    let p = if raw == "~" {
        home.to_path_buf()
    } else if let Some(p) = raw.strip_prefix("~/") {
        home.join(p)
    } else {
        let p = Path::new(raw);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.join(p)
        }
    };
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
        || p.to_string_lossy().chars().any(char::is_control)
    {
        return Err(bad("unsafe directory"));
    }
    let p = p.canonicalize()?;
    if !p.is_dir() || p.parent().is_none() {
        return Err(bad("directory/root rejected"));
    }
    Ok(p)
}
#[derive(Clone)]
pub struct ContextSources {
    pub custom_root: PathBuf,
    pub custom_skill: Option<PathBuf>,
    pub templates_root: Option<PathBuf>,
    pub extra: String,
}
impl ContextSources {
    pub fn augment(&self, adapter: &mut AdapterConfig) {
        if !adapter.follow_folder || adapter.contract.read_dirs.is_empty() {
            return;
        }
        let mut roots = vec![self.custom_root.clone()];
        if let Some(p) = &self.templates_root {
            roots.push(p.clone());
        }
        if let Some(p) = &self.custom_skill {
            if let Some(parent) = p.parent() {
                roots.push(parent.to_path_buf());
            }
        }
        for root in roots {
            if root.is_dir() && root.parent().is_some() {
                let value = root.to_string_lossy().into_owned();
                if !adapter.contract.read_dirs.contains(&value) {
                    adapter.contract.read_dirs.push(value.clone());
                    adapter.contract.read_rules.push(format!("/{value}"));
                }
            }
        }
    }
    pub fn compose(&self, folder: &Path) -> io::Result<String> {
        let where_ = self
            .custom_skill
            .as_ref()
            .map_or_else(String::new, |p| format!(" ({})", p.display()));
        let models = self.templates_root.as_ref().map_or_else(String::new, |p| {
            format!(
                " (os modelos que vêm com o HUD, só para ler, ficam em {})",
                p.display()
            )
        });
        let custom=format!("Você está dentro do HUD de terminal. Para mudar o layout do HUD, siga a skill hud-custom{where_}; as customizações ficam em {}{models}. Dentro do HUD não grave arquivos: responda com um bloco ```hud-custom nome=<nome> arquivo=<arquivo> por arquivo. Nesta prévia Rust, salvar/importar propostas de customização pela conversa ainda não está disponível; apresente somente a proposta para revisão.",self.custom_root.display());
        let mut context=format!("{custom} Para criar projetos ou documentação na pasta do HUD ({}), siga a skill projeto-docs: dentro do HUD responda com um bloco ````hud-doc arquivo=\"<caminho relativo à pasta>\" por arquivo .md, sem gravar nada, e diga ao usuário para usar doc from-agent, doc preview INDEX e doc save ou doc overwrite, com aprovação separada.",folder.display());
        if !self.extra.is_empty() {
            context.push_str("\n\n");
            context.push_str(&self.extra);
        }
        if !hud_resources::safe_prompt(&context)
            || context.chars().count() > crate::agent_app::CONTEXT_BUDGET
        {
            return Err(bad("context unsafe or oversized; no truncation"));
        }
        Ok(context)
    }
}
pub struct LoadedSettings {
    pub adapters: Vec<AdapterConfig>,
    pub enabled: Vec<bool>,
    pub environment: BTreeMap<String, String>,
    pub context: ContextSources,
}
impl LoadedSettings {
    pub fn load(path: &Path, base: &Path, home: &Path) -> io::Result<Self> {
        let bytes = hud_resources::read_owned_file(path, 64 * 1024)?;
        let source = if path.extension().is_some_and(|s| s == "toml") {
            hud_toml::parse(&bytes, 64 * 1024)?
        } else {
            serde_json::from_slice(&bytes).map_err(|_| bad("invalid settings JSON"))?
        };
        Self::parse(&source, base, home)
    }
    pub fn from_value(source: &Value, base: &Path, home: &Path) -> io::Result<Self> {
        Self::parse(source, base, home)
    }
    fn parse(source: &Value, base: &Path, home: &Path) -> io::Result<Self> {
        let root = source
            .as_object()
            .ok_or_else(|| bad("settings object required"))?;
        if root.keys().any(|k| {
            !["claude", "codex", "opencode", "context", "environment"].contains(&k.as_str())
        }) {
            return Err(bad("unknown root setting"));
        }
        let mut adapters = Vec::new();
        let mut enabled = Vec::new();
        for (name, provider) in [
            ("claude", Provider::Claude),
            ("codex", Provider::Codex),
            ("opencode", Provider::OpenCode),
        ] {
            let row = source
                .get(name)
                .ok_or_else(|| bad("all three provider tables required"))?;
            let obj = row
                .as_object()
                .ok_or_else(|| bad("provider table required"))?;
            if obj.keys().any(|k| {
                ![
                    "enabled",
                    "tools",
                    "executable",
                    "cwd",
                    "model",
                    "timeout",
                    "profile",
                    "follow_folder",
                    "read_dirs",
                    "read_rules",
                    "max_budget_usd",
                    "full_permission_mode",
                ]
                .contains(&k.as_str())
            }) {
                return Err(bad("unknown provider setting"));
            }
            let mut cwd = directory(
                text(row, "cwd", base.to_str().ok_or_else(|| bad("base UTF8"))?)?,
                base,
                home,
            )?;
            let launch = text(row, "executable", "/unresolved/provider")?;
            if !Path::new(launch).is_absolute()
                || launch.chars().any(char::is_control)
                || Path::new(launch)
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(bad(
                    "executable must be explicit absolute path; never resolved in fake mode",
                ));
            }
            let model = text(
                row,
                "model",
                if provider == Provider::OpenCode {
                    "opencode/big-pickle"
                } else {
                    ""
                },
            )?;
            if model.starts_with('-')
                || model.len() > 200
                || !model
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:/[]-".contains(c))
            {
                return Err(bad("invalid model"));
            }
            let timeout = row
                .get("timeout")
                .map_or(Some(600.), Value::as_f64)
                .ok_or_else(|| bad("timeout numeric"))?;
            if !timeout.is_finite() || !(10.0..=3600.).contains(&timeout) {
                return Err(bad("timeout between10and3600"));
            }
            let budget = row
                .get("max_budget_usd")
                .map_or(Some(1.), Value::as_f64)
                .ok_or_else(|| bad("budget numeric"))?;
            if !budget.is_finite()
                || budget <= 0.
                || budget > 50.
                || ((budget * 100.).round() - budget * 100.).abs() > 0.00001
            {
                return Err(bad("budget must be positive cents up to50USD"));
            }
            let profile = match text(row, "profile", "leitura")? {
                "leitura" => Profile::ReadOnly,
                "completo" => Profile::Full,
                _ => return Err(bad("invalid profile")),
            };
            let follow = row.get("follow_folder").map_or(
                Ok(!obj.contains_key("cwd")
                    && !(provider == Provider::Claude && obj.contains_key("read_dirs"))),
                |v| v.as_bool().ok_or_else(|| bad("follow_folder boolean")),
            )?;
            let mut dirs = vec![cwd.to_string_lossy().into_owned()];
            if let Some(raw) = row.get("read_dirs") {
                dirs = raw
                    .as_array()
                    .filter(|v| v.len() <= 32)
                    .ok_or_else(|| bad("read_dirs array max32"))?
                    .iter()
                    .map(|v| {
                        directory(
                            v.as_str().ok_or_else(|| bad("read_dir string"))?,
                            base,
                            home,
                        )
                        .map(|p| p.to_string_lossy().into_owned())
                    })
                    .collect::<io::Result<_>>()?;
            }
            if provider == Provider::Claude && !obj.contains_key("cwd") && !dirs.is_empty() {
                cwd = PathBuf::from(&dirs[0]);
            }
            let rules = if let Some(raw) = row.get("read_rules") {
                raw.as_array()
                    .filter(|v| v.len() <= 32)
                    .ok_or_else(|| bad("read_rules array max32"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|s| !s.chars().any(char::is_control) && s.len() <= 4096)
                            .map(str::to_owned)
                            .ok_or_else(|| bad("read_rule string"))
                    })
                    .collect::<io::Result<_>>()?
            } else {
                dirs.iter().map(|d| format!("/{d}")).collect()
            };
            let mode = text(row, "full_permission_mode", "auto")?;
            if !["default", "acceptEdits", "auto", "dontAsk", "plan"].contains(&mode) {
                return Err(bad("unsupported permission mode"));
            }
            let read_tools = if let Some(raw) = row.get("tools") {
                raw.as_array()
                    .ok_or_else(|| bad("tools array required"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| bad("tool string required"))
                    })
                    .collect::<io::Result<Vec<_>>>()?
            } else {
                vec!["Read".into(), "Grep".into(), "Glob".into()]
            };
            let adapter = AdapterConfig {
                read_tools,
                contract: Config {
                    provider,
                    launch: vec![launch.into()],
                    model: model.into(),
                    read_dirs: dirs,
                    read_rules: rules,
                    budget_cents: (budget * 100.).round() as u32,
                },
                cwd,
                profile,
                timeout: Duration::from_secs_f64(timeout),
                follow_folder: follow,
                full_permission_mode: mode.into(),
                contract_version: "hud-python-contract-fixture-v1".into(),
            };
            adapter
                .prepare(&Session::default(), "validate", "", &BTreeMap::new())
                .map_err(|_| bad("provider contract rejected"))?;
            adapters.push(adapter);
            enabled.push(row.get("enabled").map_or(Ok(true), |v| {
                v.as_bool().ok_or_else(|| bad("enabled boolean"))
            })?);
        }
        let mut environment = BTreeMap::new();
        if let Some(env) = source.get("environment") {
            for (k, v) in env
                .as_object()
                .filter(|o| o.len() <= 32)
                .ok_or_else(|| bad("environment object max32"))?
            {
                if !["LANG", "OPENCODE_CONFIG_CONTENT"].contains(&k.as_str()) {
                    return Err(bad("fixture environment permits LANG and OPENCODE_CONFIG_CONTENT only; no credentials"));
                }
                let value = v
                    .as_str()
                    .filter(|s| s.len() <= 16384 && !s.contains('\0'))
                    .ok_or_else(|| bad("invalid fixture env"))?;
                environment.insert(k.clone(), value.into());
            }
        }
        let raw = source
            .get("context")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let obj = raw.as_object().ok_or_else(|| bad("context object"))?;
        if obj
            .keys()
            .any(|k| !["custom_root", "custom_skill", "templates_root"].contains(&k.as_str()))
        {
            return Err(bad("unknown context setting"));
        }
        let custom_root = directory(
            text(&raw, "custom_root", base.to_str().unwrap_or(""))?,
            base,
            home,
        )?;
        let templates_root = raw
            .get("templates_root")
            .map(|v| {
                directory(
                    v.as_str().ok_or_else(|| bad("templates_root string"))?,
                    base,
                    home,
                )
            })
            .transpose()?;
        let custom_skill = raw
            .get("custom_skill")
            .map(|v| {
                let p = PathBuf::from(v.as_str().ok_or_else(|| bad("custom_skill string"))?);
                if !p.is_absolute()
                    || p.parent() == Some(Path::new("/"))
                    || p.to_string_lossy().chars().any(char::is_control)
                    || p.components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(bad("custom_skill absolute safe path required"));
                }
                Ok(p)
            })
            .transpose()?;
        Ok(Self {
            adapters,
            enabled,
            environment,
            context: ContextSources {
                custom_root,
                custom_skill,
                templates_root,
                extra: String::new(),
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_fail_closed_and_preserve_provider_contract() {
        let base = Path::new("/tmp");
        let source = serde_json::json!({"claude":{"model":"fixture","max_budget_usd":1.25,"full_permission_mode":"plan","tools":["Read","Glob"],"profile":"completo","timeout":17},"codex":{},"opencode":{},"environment":{"OPENCODE_CONFIG_CONTENT":"{\"fixture\":true}"}});
        let loaded = LoadedSettings::parse(&source, base, base).unwrap();
        assert_eq!(loaded.adapters[0].contract.budget_cents, 125);
        assert_eq!(loaded.adapters[0].read_tools, vec!["Read", "Glob"]);
        assert_eq!(loaded.adapters[0].timeout, Duration::from_secs(17));
        assert!(loaded.adapters[0].follow_folder);
        assert_eq!(loaded.adapters[0].full_permission_mode, "plan");
        for (key, value) in [
            ("timeout", serde_json::json!(9)),
            ("model", serde_json::json!("--injection")),
            (
                "full_permission_mode",
                serde_json::json!("bypassPermissions"),
            ),
            ("max_budget_usd", serde_json::json!(1.234)),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut invalid = source.clone();
            invalid["claude"][key] = value;
            assert!(
                LoadedSettings::parse(&invalid, base, base).is_err(),
                "{key}"
            );
        }
        let mut invalid = source;
        invalid["environment"]["OPENAI_API_KEY"] = serde_json::json!("fixture");
        assert!(LoadedSettings::parse(&invalid, base, base).is_err());
    }
}
