# 🌱 Sprout
> ... because "just works" beats "highly configurable" every single time.

Sprout is a symlink and source dependency manager that just works.
It's boring in the best possible way.

All I wanted was to organize my dotfiles and tools. Then I blinked, and
suddenly I had a new Rust CLI and very strong opinions about symlinks. Could've
gone outside, touch grass. Instead I implemented `ln -s`, but in fancy and with
unit tests and colors.

You might be wondering: Why build yet another symlink tracker and source-based
dependency manager? Why reinvent the wheel, the wrench, and the garage it lives
in? Well...

- Ansible wants to SSH into your toaster to symlink a config.
- Chezmoi dreams of templating your entire life based on your hostname and the
moon phase.
- Homebrew gives up if you want a specific Git commit or gasp a local build.
- Nix manages everything. Including your free time.
- GNU Make can technically do this. But now you're writing Makefiles for your
dotfiles.

Sprout is simpler! It stays out of your way and does exactly what you ask:
track configs, fetch sources, and builds dependencies. All versioned. All in
one place. All in plain sight. No templating languages. No Turing-complete
DSLs. No server orchestration cosplay just to install Neovim.

Just you, your tools, and your ${HOME} ... sprouted by hand.

## Features

### 🔗 Dotfile Tracking & Symlinking
- Move your config files into `/sprout/symlinks`
- `sprout symlinks add [--recursive]` creates a symlink back to `$HOME`
- `sprout symlinks status [--all]` shows modifications, deletions, and optionally up-to-date files
- `sprout symlinks restore` repairs any missing or broken symlinks
- `sprout symlinks rehash` recalculates symlink hashes after manual changes
- `sprout symlinks undo <path>` removes symlinks and copies files back to original location
- Respects both `.gitignore` and `.sproutignore`

### 📦 Dependency Management & Declarative Build
- Declare Git repos or HTTP downloads (tarballs, zip files) in `manifest.sprout`
- `sprout modules fetch [package]` pulls and unpacks dependencies
- Embed shell commands and environment setup directly in `manifest.sprout`
- `sprout modules build [package] [--dry-run]` runs it in context (or just prints a ready‑to‑run script)
- `sprout modules install [package]` fetches and builds in one step
- `sprout modules status [--expand] [--all]` shows module status with build information and dependencies
- `sprout modules hash [-i]` computes and displays/updates module hashes
- `sprout modules clean [--dry-run]` removes unused cache/source directories
- Versioned directories and optional SHA256 checks for archives

### 🌍 Environment Management
- Declare environment variables (PATH, LD_LIBRARY_PATH, etc.) directly in `manifest.sprout`
- Create named environment sets to group dependencies for different contexts
- `sprout env edit [environment]` interactively edit environment (toggle modules)
- `sprout env list [environment]` list environment sets and their modules
- `sprout env generate [environment]` generate environment export statements for a specific set

### 🚀 Quick Setup
1. Initialize a new sprout directory with example modules (defaults to `/sprout`)

```bash
sprout init --empty [path]
```

2. Add the following content to your manifest:

```sprout
module cmake {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    fetch {
        http = {
            url = https://github.com/Kitware/CMake/releases/download/v4.0.3/cmake-4.0.3-linux-x86_64.tar.gz
        }
    }
    build {
        ln -sf ${SOURCE_PATH}/cmake-4.0.3-linux-x86_64/bin ${DIST_PATH}
    }
}
module gcc {
    depends_on = []
    exports = {
        PATH = "/bin"
    }
    fetch {
        http = {
            url = https://mirrors.ibiblio.org/gnu/gcc/gcc-15.1.0/gcc-15.1.0.tar.xz
        }
    }
    build {
        cd gcc-15.1.0
        ./contrib/download_prerequisites
        mkdir -p build
        cd build
        ../configure --disable-multilib --enable-languages=c,c++ --prefix=${DIST_PATH}
        make -j8
        make install
    }
}
environments {
    default = [cmake, gcc]
}
```

3. Build and Install Modules (will format the manifest and add sha256sum):
```bash
sprout modules install cmake
sprout modules install gcc
```

4. Activate the Environment (e.g., add to your `.zshrc`):
```bash
eval "$(sprout env generate)"
```

### 🔧 Git & Maintenance
- `sprout status` shows complete status (modules, symlinks, and git)
- `sprout commit [-m "message"]` commits all changes to git
- `sprout push` pushes changes to remote git repository
- `sprout edit [path]` edits manifest.sprout with $EDITOR and validates syntax
- `sprout format [-i] [path]` verifies and reformats manifest.sprout

## Directory Layout
```
/sprout
├── symlinks/              # Tracked symlinks
│   └── .zshrc
├── sprout.lock            # Lockfile (portable hashes)
├── dist/                  # Build artifacts (install output)
│   ├── neovim/
│   ├── ...
│   └── ripgrep/
├── sources/
│   ├── git/               # Git repos (as submodules)
│   └── http/              # Extracted HTTP downloads (tarballs, zip files)
├── cache/
│   └── http/              # Cached downloads (.tar.gz, .zip, etc.)
├── manifest.sprout        # Metadata manifest (alphabetically sorted)
├── .gitignore             # Sensible defaults
├── .git                   # Your own Git repo (optional)
```

## Manifest Structure
Dependencies in `manifest.sprout` can declare environment variables they need:

**Note**: Comments in `.sprout` files start with `#`. However, Sprout may
reformat the manifest when adding/removing modules, which will remove comments.
Keep important documentation in a separate README or inline in build scripts.

See the [example manifest](./src/templates/default_manifest.sprout).

When you run `sprout env`, it generates shell export statements.

## Tree-sitter Support

Sprout includes a Tree-sitter grammar for syntax highlighting `.sprout` files.

### Setup for Neovim
```bash
cd tree-sitter-sprout
tree-sitter generate
tree-sitter build

# Copy queries to Neovim config
mkdir -p ~/.config/nvim/queries/sprout
cp queries/*.scm ~/.config/nvim/queries/sprout/

# Add to your Neovim config (init.lua)
vim.filetype.add({
  extension = { sprout = "sprout" },
})

local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.sprout = {
  install_info = {
    url = "~/git/github.com/patwie/sprout/tree-sitter-sprout",
    files = {"src/parser.c"},
  },
  filetype = "sprout",
}
```

**Note**: Neovim loads queries from `~/.config/nvim/queries/sprout/` first,
then from nvim-treesitter's cache. After updating the grammar, copy the updated
queries to both locations and restart Neovim.

### Setup for tree-sitter CLI
```bash
# Add to ~/.config/tree-sitter/config.json
{
  "parser-directories": [
    "/path/to/sprout/tree-sitter-sprout"
  ],
  "language-associations": {
    "sprout": "sprout"
  }
}

# Test highlighting
tree-sitter highlight file.sprout
```

