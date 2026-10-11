# Revisão explícita de configuração e propostas

Implementação local das AT-087, AT-088 e AT-089, em 2026-10-11. O fluxo atual integra a 0.10.0-preview.12 instalada; consulte [recibo/abertura](local-preview.pt-BR.md). A evidência original preview7 é histórica.

## Configuração

Inicie com `--import-config /caminho/config.toml --import-output /diretorio/privado/revisado.json`. Use `/config preview`, examine roots, cadências, providers, comandos e snapshot original, depois `/config apply`. Reinicie com `--reviewed-config /diretorio/privado/revisado.json`. O export é imutável: arquivo existente exige outro destino. `data_dir` é registrado como referência, sem adoção de dados pessoais. Providers originais são descritos sem resolver ou executar seus binários. `/config rollback-preview` e `/config rollback` retiram o export desta sessão mediante CAS; original e backup permanecem preservados.

## Custom

Use `--custom-root /diretorio/privado/custom --custom-selection-dir /diretorio/privado/selecao`. Uma resposta completa e bem sucedida do fake pode conter blocos `hud-custom nome=fixture arquivo=layout.toml` e assets. `/custom from-agent`, `/custom preview 1` (ou `/custom salvar`) revisam visual e conteúdo; Esc restaura o estado anterior. `/custom save` cria e aplica, `/custom overwrite` confirma substituição explicitamente. `/custom lista`, `/custom NOME` e `/custom padrao` permitem seleção persistente. `/custom rollback-preview` e `/custom rollback` restauram bundle e seleção mediante CAS. Preview usa assets da revisão e desabilita feeds; custom selecionado ou salvo não recebe confiança para executar comandos automaticamente.

## Comandos

Use `--commands-output /diretorio/privado/comandos.toml`. `/cmd` revisa o último bloco `hud-comandos` da resposta completa. `/cmd save` cria, `/cmd overwrite` substitui após revisão; a revisão apresenta argv, cwd, timeout, limites e confirmação. `/cmd rollback-preview` revisa backup ou remoção de criação; esta exige `/cmd delete-created`. Salvar/recarregar não executa comandos. Propostas também podem vir de arquivos explicitamente selecionados pelas flags `--custom-proposals` e `--command-proposals`.

## Integridade e limites

Arquivos privados, limites de tamanho, NOFOLLOW, descritores ancorados, comparação de bytes/inode e snapshots preservam edições manuais. CRLF, Unicode e wikilinks permanecem na proposta; controles inseguros impedem sua gravação e apenas a exibição é sanitizada. Falhas de fsync após publicação são comunicadas como durabilidade incerta e preservam o recibo possível. Recibos v1 privados persistem hashes e identidades; o início recupera somente metadados e exige nova revisão/confirmação antes do rollback. Versão incompatível, transação incompleta, adulteração, edição posterior e links recusam a recuperação sem restaurar dados. No custom recuperado, rollback de criação seleciona o padrão e rollback de substituição seleciona o bundle restaurado; não recupera a geometria transitória da sessão anterior. A troca de pasta conserva a cadência importada.

Não houve uso de providers reais, execução dos comandos propostos, commit, push ou workflow. A instalação atual e seu rollback têm recibo próprio; esta página não autoriza nem executa nova instalação. Gates externos de providers, Windows/COSMIC e MSRV 1.88 continuam nas atividades existentes. Guardas de namespace e checksums não autenticam alterações coerentes por atacante com o mesmo UID. Interrupção entre publicação e persistência de recibo pode impedir recuperação após reinício: a operação recusa recuperação incompleta e conserva os arquivos para inspeção. Lock custom é não bloqueante, com recusa em contenção.

## Verificação

Cargo workspace test, fmt, Clippy all-targets com warnings negados e build locked/offline. Python: 495 testes passaram, 19 skips. `scripts/check_review_pty.py` usa apenas diretórios temporários privados e fake, verifica criação/revisão/cancelamento/save/rollback/restart, bytes CRLF e restauração termios. Para debug, compile com `HUD_PREVIEW_FAKE_SHA256` igual ao SHA256 do fake construído; o script copia esse fake para diretório privado, sem enfraquecer as guardas do resolver. Revisão independente dos contratos e tratamento de falhas realizada.

## Instalação e evidência

Orientações atuais estão no [guia local](local-preview.pt-BR.md). Testes preview7 (343 Rust/13 scripts e PTY) são evidência histórica do fluxo inicial; não substituem o recibo 0.10.0-preview.12 (392 workspace e PTYs do artefato atual). Fallback histórico hud-python preservado, sem orientar o runtime-alvo.
