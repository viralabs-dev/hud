> Sequência histórica: instruções antigas hud-rs separado/Python padrão não descrevem entrada Rust hud atual; consulte README corrente.

> **Registro histórico.** Não usar os comandos de instalação/rollback deste checkpoint para a prévia atual. Consulte [guia local atual](local-preview.pt-BR.md) e recibo do artefato instalado.

# Prévia 2 local — 0.2.0-preview.2

Linux x86_64, desenvolvimento, offline. Primeira prévia e Python preservados. Executável separado; fake inerte próprio autenticado pelo SHA256 compilado (`--build-info`). Nenhum provedor real habilitado.

Diferenças visíveis: abas Alt+1–4; entrada Unicode com cursor e viewport, histórico, Tab local e F1–F10 de comandos configurados; atalhos /ajuda, /c, /x, /o, /novo, /parar, /perfil leitura, /agenda, /buscar, /notas, /uso, /skills, /agentes, /doc, /custom, /sair. Fkeys preservam a confirmação do comando; completar não executa. /c,/x,/o selecionam a aba; // envia slash literal. /perfil completo permanece recusado; /novo sem argumento usa o provedor selecionado (inicial Codex), diferença explícita do fallback Claude Python.

Markdown: títulos/ênfase/código/links sem abertura automática/listas/quotes/propostas recolhidas/tabelas alinhadas por células Unicode ou empilhadas em terminal estreito. Limites: 128 Ki caracteres, 4096 linhas de entrada, 8192 linhas físicas, 256 spans por trecho inline. Sanitização antecede parsing. Não é implementação CommonMark completa.

Custom JSON e subconjunto TOML: IDs Python `sistema`, `comandos`, `uso_claude`, `uso_codex`, `saida`, `agentes`; `mostrar` e horizonte `dias` aplicados. Dias têm limite superior exclusivo, incluem atrasados como Python. Drag, geometria e seleção lembrada com proveniência; trust de command feeds separado e digest exato.

Uso e skills: leitura explícita, refresh em worker a cada 5s ou `resources refresh`; janelas, percentuais restantes, reset e idade/STALE; último snapshot válido retido em erro. Monitor refresh 3s: metadados selecionados de Codex/Claude/export OpenCode e JSON legado OpenCode (`id,directory,time.updated,parentID`); prompts/título/conteúdo não lidos na saída. Estado registrado não prova processo vivo. SQLite OpenCode, descoberta automática de projetos/processos e apresentação completamente equivalente ao Python continuam trabalho local AT-073; não encerrar paridade.

AT-070: scanner/cache/consultas/agenda/clock local/ações fixture têm regressões Linux; resta executar em ambientes nativos macOS/Windows. Scanner/store POSIX ainda recusam operações não suportadas no Windows; FFI de calendário Windows e mudanças DST precisam runner Windows real. Esse port é trabalho condicionado ao ambiente nativo, não pode ser declarado validado por compilar Linux. Notas pessoais reais não foram escritas. Persistência pode reportar durabilidade incerta após rename; corrida residual de outro processo mesmo UID permanece descrita.

macOS/Windows/ARM/MSRV1.88, versões/contratos CLI reais/Full, terminal físico e matriz AT-075 pendentes. AT-076 exige aceite de troca. Sem commit, push, Actions, publicação ou novos downloads.
