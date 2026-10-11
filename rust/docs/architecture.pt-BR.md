# Arquitetura Rust e limites da migração

[English](architecture.en.md) · [estado atual](../README.pt-BR.md)

Arquitetura-alvo Rust. `hud-domain` possui transições puras de layout/foco/arraste; `hud-layout-codec`, formato Rust; `hud-layout-store`, persistência privada. `hud-data` reúne agenda, calendário, notas, pastas e descoberta Vault limitada. `hud-custom`/`hud-toml` interpretam configuração/propostas; `hud-feeds` atualiza textos/arquivos e argv com confiança explícita. `hud-resources` lê skills/uso/metadados; `hud-metrics` isola amostragem por SO. Esses crates não escolhem defaults pessoais.

`hud-agent-protocol` define adapters provider/versão/perfil e contratos assinatura/plataforma. `hud-agent-process` supervisiona workers/streaming, eventos limitados, árvores de processos e gates versão/auth. `hud-runner` executa argv literal com ambiente mínimo/PATH fixo. `hud-doc-store` aplica documentos aprovados com descritores verificados. `hud-tui` coordena casos de uso, confirmação, abas, Unicode e Ratatui/Crossterm.

Traits pequenos atendem implementações alternativas e testes de falha, sem abstração por struct. Domínio é determinístico; adaptadores realizam alterações de arquivos/processos mediante aceite explícito. Bibliotecas e parsers com fixtures não comprovam suporte nativo.

Fluxo: caminhos explícitos → parse privado limitado → estado/proposta → revisão visível → aprovação distinta → worker/store → eventos limitados ou reconciliação → render. Gravar config/custom/docs/comandos não autoriza execução. Capabilities fake/produção não são intercambiáveis; mudar pasta/perfil/backend revoga propostas e sessões pertinentes.

Descritores POSIX ancoram pastas e recusam links; rename/sync tratam durabilidade explicitamente. Erro de sync após rename pode ocorrer com alteração já publicada: reconciliar, sem retry cego. Comparação de bytes/inode detecta edições manuais, sem prometer CAS perfeito contra mesmo UID. Storage Windows nativo continua necessário; fixture de política não o entrega.

Ordem/aceite em [paridade](parity.pt-BR.md) e [suporte](support.pt-BR.md). Runtime e entrada principal hud são Rust após cutover verificado. Fonte/settings/dados legados permanecem separados, com fallback explícito hud-python. Importação é opcional/revisada e rollback usa recibo/CAS; helpers de desenvolvimento não são runtime.

Workers separados tratam descoberta PATH por metadados e preparação/hash de backend padrão, sem consulta login/modelo na abertura. Found/versão declarada/auth desconhecida não autorizam processo. Preparação assíncrona mantém UI disponível; calls filesystem remotos não têm cancelamento kernel garantido.
