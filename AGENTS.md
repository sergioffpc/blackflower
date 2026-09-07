# Convenções do projeto

## Commits

Todos os novos commits devem seguir [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

- Formato: `tipo(escopo opcional): descrição`.
- Usa `feat` para funcionalidades novas e `fix` para correções de erros.
- Para outras alterações, usa um tipo adequado, como `docs`, `refactor`, `test`, `chore`, `build`, `ci`, `perf` ou `style`.
- O escopo é opcional; quando presente, identifica a área alterada entre parênteses, por exemplo `fix(api): corrigir validação`.
- Assinala alterações incompatíveis com `!` antes de `:` (por exemplo `feat(api)!: remover endpoint antigo`) ou com um rodapé `BREAKING CHANGE: descrição da incompatibilidade`.
- Se incluíres corpo ou rodapés, separa-os da secção anterior com uma linha em branco.
- Antes de criar um commit, verifica se a mensagem respeita esta convenção e descreve as alterações incluídas.

Exemplos: `feat: adicionar autenticação`, `fix: corrigir cálculo do total`, `docs: atualizar instruções de instalação`.
