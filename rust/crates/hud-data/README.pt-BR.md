# Fase de dados do HUD em Rust

O módulo oferece agenda pura em memória e snapshot do Vault somente para leitura, usando raiz explícita. Não lê configuração pessoal automaticamente, não grava arquivos implicitamente, não segue links, não inicia processos nem threads.

Agenda: datas pt-BR, horários, Markdown compatível com as linhas Python, adicionar/remover/concluir/podar, ordenação de pendências e rótulos de dias. A data atual é fornecida pelo chamador. O scanner POSIX usa descritores de diretório fixados e NOFOLLOW em todos os componentes; notas usam NONBLOCK, validação de arquivo regular, dispositivo/inode, UID atual e limite de bytes, inclusive crescimento durante leitura. Limites adicionais cobrem total, profundidade, quantidade e busca.

Lê Kanban, tarefas com vencimento, nomes de conflito e busca literal sem distinção de caixa. Identidades de caminhos permanecem cruas; a interface deve usar display_rel/display_source ou clean_line para apresentar qualquer caminho externo, inclusive conflitos.

A exigência de UID nas notas é mais restrita que hud/vault.py atual, que valida tipo/identidade/tamanho sem exigir proprietário. Diretórios podem ser compartilhados: não há escrita. Fixar descritores evita redirecionamento por links, mas não impede mover um diretório já aberto ou modificar o mesmo inode aberto; não promete snapshot transacional.

Faltam watcher/cache/refresh, estatísticas de recentes/hoje/Obsidian, todas as bordas dos parsers e limpeza Python, ações de notas e adaptador Windows. Não-Unix retorna Unsupported; macOS não foi compilado. Não declarar paridade completa.

Fixtures locais testam datas/Unicode/operações de agenda, Kanban/vencimentos, limpeza de escapes/bidi, conteúdo preservado, links, corrida com novo inode/link, FIFO e limites. Nenhum Vault ativo foi usado nos testes. A documentação inglesa detalha contratos e lacunas.

AgendaStore/FileAgendaStore permitem persistência POSIX opt-in em diretório explícito, arquivo agenda.md separado. Limites: 256 KiB e 2.048 itens. Diretório/arquivo devem ser privados e do UID atual; componentes usam NOFOLLOW e ancestrais graváveis por terceiros sem sticky são rejeitados. Load ausente não cria pasta. Save valida antes de criar, grava temporário exclusivo 0600, sincroniza, renomeia atomicamente e sincroniza diretório. Falha antes do rename preserva estado anterior e remove temporário; falha de fsync após rename pode deixar a nova versão visível, exigindo reload/reconciliação na aplicação. Não promete rollback nesse caso.

UTF-8 inválido, linhas de tarefa malformadas, arquivo público/link/FIFO e excesso de limites são recusados. Clock armazenado inválido é recusado, mais restrito que regex de load Python. Prosa/comentários tolerados na entrada não são preservados na serialização canônica. Itens originados do Vault não são aceitos como agenda local. Não-Unix falha sem escrita.

## Cache, calendário local e ações explícitas de notas

`ScanCache`/`scan_cached` reaproveitam texto e parse de notas sem mudanças de dispositivo/inode/UID/tamanho/mtime/ctime e nanossegundos. Todas as notas continuam abertas/validadas por descritor; cache não ignora NOFOLLOW, proprietário, limites ou remoções. Retrato expõe hoje, 12 recentes, presença de .obsidian, leituras/hits de cache; filtros boards_matching/tasks_on/recent_matching. Rejeições por proprietário/tamanho ficam fora de recentes/hoje, diferença em relação aos metadados Python. Não há isolamento transacional de snapshot contra alterações do mesmo inode indistinguíveis ou posteriores à validação.

CalendarClock injeta dia e meia-noite locais. SystemLocalClock usa libc 0.2.190 já travada, localtime_r/mktime com DST recalculado à meia-noite, sem shell/processo/mudança global de TZ. FakeClock testa dias de 23/25h; conversão nativa foi validada somente no host Linux atual, sem transição DST real. Fora de Unix retorna Unsupported.

NoteStore exige raiz explícita. preview_read apenas lê; preview_toggle cria proposta de mudar um checkbox na linha física começando em 1, preservando CRLF/Unicode/wikilinks/aliases/prosa. apply_toggle exige chamada distinta de aprovação e verifica bytes/inode/timestamps/permissões/proprietário/cadeia inteira de pastas antes e logo antes do rename. Reabre a cadeia selecionada no precommit; pasta movida durante a escrita do temporário faz recusar e preservar notas antiga/nova. Recusa links/FIFO/traversal/arquivos de outro UID e pasta/nota gravável por grupo/outros; temporário exclusivo privado, sync, substituição e sync da pasta. Prévia ou leitura não escreve nem abre GUI.

A verificação é otimista, não compare-and-swap atômico de filesystem: ator autorizado a alterar a pasta pode disputar o intervalo final de verificação/rename ou mover a pasta aberta. Erro de fsync após rename deixa resultado/durabilidade incertos; integração deve reler e reconciliar, sem alegar rollback. Testes somente em fixtures incluem edição manual antes/durante commit, troca de inode/pasta, preservação dos bytes e limpeza de temporário. Nenhuma nota pessoal foi modificada.
