# doer

A vim-flavoured terminal todo app built with Elixir.

## Install

### Homebrew

```sh
brew install armanarutiunov/doer/doer
```

### From source

Requires Elixir and Erlang.

```sh
git clone https://github.com/armanarutiunov/doer.git
cd doer
mix deps.get
mix escript.build
mv doer /usr/local/bin/
```

## Keybindings

### Normal mode

| Key | Action |
|-----|--------|
| `j` / `k` / arrows | Navigate |
| `a` | Add todo |
| `e` / `i` | Edit todo |
| `d` | Delete todo |
| `space` | Toggle done |
| `v` | Enter visual mode |
| `/` | Search |
| `G` / `g` | Jump to end / start |
| `ctrl+d` / `ctrl+u` | Half page down / up |
| `?` | Toggle help |
| `q` | Quit |

### Visual mode

| Key | Action |
|-----|--------|
| `j` / `k` | Extend selection |
| `J` / `K` | Reorder selected |
| `d` | Delete selected |
| `space` | Toggle selected |
| `escape` | Exit visual mode |

### Search mode

| Key | Action |
|-----|--------|
| type | Filter todos |
| `enter` | Navigate results |
| `escape` | Cancel search |

## Data

Todos are persisted as JSON at `~/.doer/todos.json`.
