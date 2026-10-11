//! Built-in geometry ported from hud/layout.py DEFAULT; no runtime file/config reads.
use std::io;
const TOML: &str = r#"
nome='padrao'
descricao='Sistema, comandos, agenda e uso; Vault e saida'
[[coluna]]
largura=36
paineis=[{id='sistema',altura=11},'comandos',{id='agenda',peso=1},{id='uso_claude',altura=4},'uso_codex']
[[coluna]]
paineis=[{id='vault',peso=55},{id='saida',peso=45}]
[[bloco]]
id='sistema'
titulo='SISTEMA'
[[bloco]]
id='comandos'
titulo='COMANDOS'
[[bloco]]
id='agenda'
titulo='AGENDA'
[[bloco]]
id='uso_claude'
titulo='USO CLAUDE'
[[bloco]]
id='uso_codex'
titulo='USO CODEX'
[[bloco]]
id='vault'
titulo='VAULT'
[[bloco]]
id='saida'
titulo='SAÍDA'
"#;
pub fn custom() -> io::Result<hud_custom::Custom> {
    hud_custom::Custom::parse_toml(TOML.as_bytes(), "padrao", std::path::Path::new("/"))
}
pub fn geometry_custom(custom: &hud_custom::Custom, width: u16) -> hud_custom::Custom {
    let mut result = custom.clone();
    // Python DEFAULT limits the left column to 38..56 terminal cells.
    if let Some(column) = result.columns.first_mut() {
        let cells = (f64::from(width) * 0.36).round_ties_even().clamp(38., 56.);
        column.width = Some(cells * 100. / f64::from(width.max(1)));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_has_a_fixed_name_without_source_path_resolution() {
        assert_eq!(custom().unwrap().name, "padrao");
    }
    #[test]
    fn python_default_geometry_matches_verified_reference_sizes() {
        let refs: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/python-default-geometry.json")).unwrap();
        let auto =
            std::collections::BTreeMap::from([("commands".into(), 2), ("usage_codex".into(), 3)]);
        for reference in refs.as_array().unwrap() {
            let width = reference["width"].as_u64().unwrap() as u16;
            let height = reference["height"].as_u64().unwrap() as u16;
            let rects = geometry_custom(&custom().unwrap(), width)
                .geometry(width, height - 3, &auto)
                .unwrap();
            assert_eq!(rects.len(), 7);
            for rect in rects {
                let python_id = match rect.id.as_str() {
                    "system" => "sistema",
                    "commands" => "comandos",
                    "usage_claude" => "uso_claude",
                    "usage_codex" => "uso_codex",
                    "output" => "saida",
                    id => id,
                };
                let expected = &reference["rects"][python_id];
                let values = [
                    rect.rect.row,
                    rect.rect.col,
                    rect.rect.height,
                    rect.rect.width,
                ];
                let expected: Vec<u16> = expected
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect();
                assert_eq!(
                    values.as_slice(),
                    expected.as_slice(),
                    "{width}x{height} {python_id}"
                );
            }
        }
    }
}
