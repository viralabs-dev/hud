# Actions dos workflows fixadas em commit

Os workflows (`.github/workflows/ci.yml` e `release.yml`) usam actions de
terceiros. Uma tag como `v4` pode ser movida a qualquer momento pelo dono da
action, e o próximo build rodaria código diferente sem nenhuma mudança neste
repositório. Por isso toda action é fixada no SHA de 40 hexadecimais do commit
da tag oficial, com a versão exata num comentário ao lado:

```yaml
- uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0
```

O SHA é o que vale; o comentário só diz a que versão ele corresponde, para quem
lê e para a conferência. Além disso:

- todo workflow declara `permissions: contents: read` no topo, e só o job
  `release` sobe para `contents: write`;
- todo `actions/checkout` tem `persist-credentials: false`, para o token não
  ficar gravado no `.git/config` do runner;
- o `tests/test_workflows.py` falha se algum `uses:` não estiver nesse formato,
  se algum checkout persistir a credencial, ou se a tabela abaixo não bater com
  os workflows.

Assinatura e notarização dos binários não fazem parte deste documento.

## Versões em uso

| Action | Versão | SHA do commit | Onde |
|---|---|---|---|
| `actions/checkout` | `v4.4.0` | `11d5960a326750d5838078e36cf38b85af677262` | todos os jobs |
| `actions/setup-python` | `v5.6.0` | `a26af69be951a213d495a4c3e4e4022e16d87065` | CI `test` e `binary`; Release `test` e `build` |
| `actions/upload-artifact` | `v4.6.2` | `ea165f8d65b6e75b540449e92b4886f43607fa02` | Release `build` |
| `actions/download-artifact` | `v4.3.0` | `d3f86a106a0bac45b974a628896c90dbdf5c8093` | Release `release` |

Cada versão é a mais nova da mesma major que os workflows já usavam (`@v4`,
`@v5`), então o comportamento dos jobs não mudou. Na data da fixação, a tag
major (`v4`, `v5`) apontava para o mesmo commit da versão exata.

## Como conferir

Com o `gh` autenticado (só leitura na API):

```bash
python3 scripts/conferir-actions.py
```

Para cada action, o script resolve a tag da versão no repositório oficial da
action, segue tag anotada até o commit, compara com o SHA fixado, confere que o
GitHub marca o commit como assinado e verificado, e avisa se há versão mais nova
da mesma major. Sai com 1 se algo não bater.

À mão, para uma action:

```bash
gh api repos/actions/checkout/git/ref/tags/v4.4.0 --jq '.object.type + " " + .object.sha'
# se o tipo for "tag" (tag anotada), siga até o commit:
gh api repos/actions/checkout/git/tags/<sha-da-tag> --jq '.object.type + " " + .object.sha'
# o commit tem assinatura verificada:
gh api repos/actions/checkout/commits/<sha-do-commit> --jq '.commit.verification.reason'
```

O SHA final (tipo `commit`) tem que ser o mesmo do workflow, e a verificação
tem que dar `valid`.

## Como atualizar

1. Rode `python3 scripts/conferir-actions.py` e veja quais actions têm versão
   nova (`(há vX.Y.Z)`). Leia as notas da release no repositório da action.
2. Resolva o commit da versão nova com os comandos `gh api` acima. Nunca copie
   o SHA de um blog, de um PR de terceiros ou de outro repositório.
3. Troque o SHA e o comentário em **todos** os `uses:` daquela action nos dois
   workflows (o teste exige o mesmo SHA para a mesma action).
4. Atualize a tabela acima com a versão e o SHA novos.
5. Rode `python3 scripts/conferir-actions.py` e
   `python3 -m unittest discover -s tests -t .`; os dois têm que passar.
6. Mudança de major (ex.: `checkout` v4 → v5) é uma decisão à parte: leia o
   que mudou, porque pode mudar o que o job faz (versão do Node, entradas
   novas, padrões diferentes), e deixe o CI rodar no PR antes do merge.
