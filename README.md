# json-lint

A fast Rust JSON linter and fixer, inspired by
[json-linter](https://github.com/atomicptr/json-linter). It is distributed as a
prebuilt binary on PyPI, the same way as ruff.

```bash
pip install fast-json-lint
json-lint my-file.json some-dir/
```

For every file (or every `*.json` file in a given directory) it:

- sorts object keys recursively in natural order (`a2` before `a10`)
- rewrites the file with 2-space indentation and a trailing newline
- reports files that are not valid JSON and leaves them unchanged

It exits with status 1 if any file fails.

## pre-commit

```yaml
repos:
  - repo: https://github.com/Kanzey/json-lint
    rev: v0.1.1
    hooks:
      - id: json-lint
        # files: ^config/   # optionally limit which JSON files are touched
```

The hook runs on all `*.json` files. pre-commit builds it with cargo the first time
and caches it, installing a Rust toolchain if one is missing. Because the hook rewrites
files, the first run fails if anything changed: review the changes, `git add` them, and
commit again.
