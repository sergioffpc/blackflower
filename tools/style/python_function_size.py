"""Check Python implementation body lengths without counting documentation."""

import ast
import io
import pathlib
import sys
import tokenize

MAX_BODY_LINES = 40


def _documentation_lines(tree: ast.AST, text: str) -> set[int]:
    excluded = {
        token.start[0]
        for token in tokenize.generate_tokens(io.StringIO(text).readline)
        if token.type == tokenize.COMMENT
        and not token.line[: token.start[1]].strip()
    }
    for node in ast.walk(tree):
        if (
            not isinstance(
                node,
                (
                    ast.Module,
                    ast.ClassDef,
                    ast.FunctionDef,
                    ast.AsyncFunctionDef,
                ),
            )
            or not node.body
        ):
            continue
        first = node.body[0]
        if (
            isinstance(first, ast.Expr)
            and isinstance(first.value, ast.Constant)
            and isinstance(first.value.value, str)
        ):
            excluded.update(
                range(first.lineno, (first.end_lineno or first.lineno) + 1)
            )
    return excluded


def check(path: pathlib.Path) -> bool:
    """Reports oversized function bodies; returns whether the file conforms."""
    with tokenize.open(path) as source:
        text = source.read()
    tree = ast.parse(text, filename=str(path))
    excluded = _documentation_lines(tree, text)
    valid = True
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        body_lines = set(
            range(node.body[0].lineno, (node.end_lineno or node.lineno) + 1)
        )
        size = len(body_lines - excluded)
        if size > MAX_BODY_LINES:
            print(
                f"{path}:{node.lineno}: {node.name} has {size} implementation "
                f"lines (maximum {MAX_BODY_LINES})"
            )
            valid = False
    return valid


def main() -> int:
    """Checks the supplied Python source paths and returns a process status."""
    results = [check(pathlib.Path(argument)) for argument in sys.argv[1:]]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
