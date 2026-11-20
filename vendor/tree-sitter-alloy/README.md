# tree-sitter-alloy

A [tree-sitter](https://tree-sitter.github.io/) grammar for the
[Alloy 6](https://alloytools.org/) formal specification language.

Covers the full Alloy 6 syntax including temporal operators (`always`,
`eventually`, `until`, `since`, etc.), `var` fields, and trace-style `run`
commands.

## Usage

### Neovim

Add the parser to your tree-sitter config via
[nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter).
A `highlights.scm` query is included in `queries/`.

| Neovim version | ABI | Tag      |
|----------------|-----|----------|
| 0.11+          | 15  | `main`   |
| 0.9–0.10       | 14  | `abi-14` |
| 0.8 or older   | 13  | `abi-13` |

### Emacs (29+)

Emacs links against your system's `libtree-sitter`, and the grammar must be
compiled for the matching ABI version. First, check which version you have:

macOS (Homebrew):
```bash
brew info tree-sitter
```

Linux / BSD:
```bash
pkg-config --modversion tree-sitter
```

Then pick the right branch from the table below and add it to your config:

| libtree-sitter | ABI | Tag      |
|----------------|-----|----------|
| 0.25+          | 15  | `main`   |
| 0.21–0.24      | 14  | `abi-14` |
| 0.20           | 13  | `abi-13` |

```elisp
;; For libtree-sitter 0.25+ (ABI 15) — use main (the default):
(add-to-list 'treesit-language-source-alist
             '(alloy "https://github.com/vincentlu/tree-sitter-alloy"))

;; For older versions — specify the matching tag, e.g. abi-14:
(add-to-list 'treesit-language-source-alist
             '(alloy "https://github.com/vincentlu/tree-sitter-alloy" "abi-14"))
```

Then run `M-x treesit-install-language-grammar` and select `alloy`.

### Helix

Place the compiled grammar and `queries/highlights.scm` in
`runtime/grammars/sources/alloy/` and add an entry to `languages.toml`.

## Development

```bash
npm install
npx tree-sitter generate
npx tree-sitter test
```

## Testing

The test corpus (`test/corpus/`) contains 83 tests covering all grammar rules:
declarations, expressions, temporal operators, comparisons, literals, names,
precedence, and quantifiers.

The grammar has also been validated against 331 real-world `.als` files from the
Alloy examples, [AlloyTools/models](https://github.com/AlloyTools/models), and
other public repositories (100% parse rate).

## References

- [Alloy documentation](https://alloy.readthedocs.io/en/latest/)
- Daniel Jackson, [*Software Abstractions*](https://mitpress.mit.edu/9780262528900/software-abstractions/) (MIT Press)
- [AlloyTools](https://github.com/AlloyTools/org.alloytools.alloy)

## License

[MIT](LICENSE)
