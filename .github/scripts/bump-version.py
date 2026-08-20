import re, sys, pathlib

version = sys.argv[1]

root = pathlib.Path("Cargo.toml")
text = root.read_text()
new, n = re.subn(
    r'(?m)^(\[workspace\.package\][^\[]*?^version = ")[^"]*(")',
    lambda m: m.group(1) + version + m.group(2),
    text,
    count=1,
    flags=re.DOTALL,
)
if n != 1:
    sys.exit("could not find the workspace version in Cargo.toml")
root.write_text(new)

cli = pathlib.Path("cli/Cargo.toml")
text = cli.read_text()
new, n = re.subn(
    r'(doer-core = \{ path = "\.\./core", version = ")[^"]*(")',
    lambda m: m.group(1) + version + m.group(2),
    text,
    count=1,
)
if n != 1:
    sys.exit("could not find the pinned doer-core version in cli/Cargo.toml")
cli.write_text(new)
print(f"bumped to {version}")
