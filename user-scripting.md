# User Scripting in Sprout

## Overview

Sprout supports custom subcommands via executable scripts, giving users full flexibility to:
- **Extend existing modules** (e.g., add package to go-tools' depends_on list)
- **Create new standalone modules** (e.g., add ripgrep as separate module)
- **Automate workflows** (e.g., update all modules, batch operations)

Scripts can manipulate the manifest programmatically using the `sprout-edit` helper command.

## Custom Commands

### Location
Place executable scripts in: `~/.config/sprout/commands/`

### Naming Convention
Scripts must be named: `sprout-<command>`

### Usage
```bash
# Script: ~/.config/sprout/commands/sprout-go
# Run as: sprout go add gopls

# Script: ~/.config/sprout/commands/sprout-rust
# Run as: sprout rust add ripgrep --standalone
```

### Environment Variables
Scripts receive:
- `SPROUT_PATH` - Path to sprout directory (e.g., `/sprout`)
- `SPROUT_MANIFEST` - Path to manifest.sprout
- `SPROUT_LOCKFILE` - Path to .sproutlock

## The `sprout-edit` Helper Command

Powerful built-in command for safe manifest manipulation.

### Commands

#### Add New Module
```bash
sprout-edit add-module <name> \
    --git <url> [--ref <tag>] [--recursive] \
    --build <command> [--build <command>...] \
    --export <VAR>=<path> [--export <VAR>=<path>...] \
    --depends-on <module> [--depends-on <module>...]
```

#### Append to List Field
```bash
sprout-edit append <module> <field> <value>
# Examples:
sprout-edit append gcc depends_on binutils
sprout-edit append rust-tools depends_on ripgrep
```

#### Append Build Command
```bash
sprout-edit append-build <module> <command>
# Example:
sprout-edit append-build go-tools "go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest"
```

#### Add Export
```bash
sprout-edit add-export <module> <VAR> <path>
# Example:
sprout-edit add-export gcc LD_LIBRARY_PATH "/lib32"
```

#### Set Field Value
```bash
sprout-edit set <module> <field> <value>
# Example:
sprout-edit set ripgrep name rg
```

#### Query Manifest
```bash
sprout-edit get <module> <field>
# Examples:
sprout-edit get go-tools depends_on  # Returns list
sprout-edit get gcc exports          # Returns exports map
```

#### Check if Module Exists
```bash
sprout-edit exists <module>
# Exit code 0 if exists, 1 if not
```

#### Remove Module
```bash
sprout-edit remove-module <name>
```

## Example Scripts

### Strategy 1: Extend Existing Module

**~/.config/sprout/commands/sprout-go**
```bash
#!/bin/bash
# Add Go tool to go-tools module or create standalone

set -e

ACTION=$1
PACKAGE=$2
VERSION=${3:-latest}
STANDALONE=false

# Parse flags
shift 2
while [ $# -gt 0 ]; do
    case "$1" in
        --standalone) STANDALONE=true ;;
        --version) VERSION="$2"; shift ;;
    esac
    shift
done

if [ "$ACTION" != "add" ]; then
    echo "Usage: sprout go add <package> [--version <ver>] [--standalone]"
    exit 1
fi

if [ -z "$PACKAGE" ]; then
    echo "Error: Package name required"
    exit 1
fi

NAME=$(basename $PACKAGE)

if [ "$STANDALONE" = true ]; then
    echo "Creating standalone module for $NAME..."
    
    sprout-edit add-module "$NAME" \
        --git "https://$PACKAGE" \
        --build "go build -o \${DIST_PATH}/$NAME" \
        --export PATH=/
    
    echo "Created standalone module: $NAME"
    echo "Run: sprout modules install $NAME"
else
    # Add to go-tools module by appending build command
    echo "Adding $PACKAGE to go-tools module..."
    
    # Check if go-tools exists
    if ! sprout-edit exists go-tools; then
        echo "Error: go-tools module not found"
        echo "Create it first or use --standalone"
        exit 1
    fi
    
    # Append go install command to build block
    sprout-edit append-build go-tools "go install ${PACKAGE}@${VERSION}"
    
    echo "Added $PACKAGE@$VERSION to go-tools"
    echo "Run: sprout modules build go-tools --rebuild"
fi
```

**Usage:**
```bash
# Add to go-tools module (appends build command)
sprout go add golang.org/x/tools/gopls
sprout go add github.com/golangci/golangci-lint/cmd/golangci-lint --version v1.50.0

# Create standalone module
sprout go add github.com/junegunn/fzf --standalone
```

**Result in manifest:**
```
module go-tools {
    depends_on = [go]
    exports = {
        PATH = "/bin"
    }
    build {
        env {
            GOBIN = "${DIST_PATH}/bin"
            GOPROXY = "direct"
            PATH = "${SPROUT_DIST}/go/bin:${PATH}"
        }
        go install github.com/jesseduffield/lazygit@v0.55.1
        go install golang.org/x/tools/gopls@latest
        go install github.com/golangci/golangci-lint/cmd/golangci-lint@v1.50.0  # <-- Added
    }
}
```

### Strategy 2: Smart Detection

**~/.config/sprout/commands/sprout-rust**
```bash
#!/bin/bash
# Add Rust crate - auto-detect if should extend or create new

set -e

ACTION=$1
CRATE=$2
VERSION=${3:-latest}

if [ "$ACTION" != "add" ]; then
    echo "Usage: sprout rust add <crate> [version] [--standalone]"
    exit 1
fi

# Check if rust-tools module exists
if sprout-edit exists rust-tools; then
    # Ask user
    echo "rust-tools module exists."
    echo "1) Add to rust-tools (appends cargo install command)"
    echo "2) Create standalone module"
    read -p "Choice [1]: " choice
    choice=${choice:-1}
    
    if [ "$choice" = "1" ]; then
        sprout-edit append-build rust-tools "cargo install $CRATE --version $VERSION"
        echo "Added $CRATE to rust-tools"
        echo "Run: sprout modules build rust-tools --rebuild"
    else
        sprout-edit add-module "$CRATE" \
            --git "https://github.com/rust-lang/$CRATE" \
            --ref "v$VERSION" \
            --build "cargo build --release" \
            --build "cargo install --path . --root \${DIST_PATH}" \
            --export PATH=/bin
        
        echo "Created standalone module: $CRATE"
        echo "Run: sprout modules install $CRATE"
    fi
else
    # No rust-tools, create standalone
    sprout-edit add-module "$CRATE" \
        --git "https://github.com/rust-lang/$CRATE" \
        --ref "v$VERSION" \
        --build "cargo build --release" \
        --build "cargo install --path . --root \${DIST_PATH}" \
        --export PATH=/bin
    
    echo "Created standalone module: $CRATE"
    echo "Run: sprout modules install $CRATE"
fi
```

### Strategy 3: Batch Operations

**~/.config/sprout/commands/sprout-go-batch**
```bash
#!/bin/bash
# Add multiple Go tools at once

set -e

if [ "$1" != "add" ]; then
    echo "Usage: sprout go-batch add <package1> <package2> ..."
    exit 1
fi

shift  # Remove 'add'

# Check if go-tools exists
if ! sprout-edit exists go-tools; then
    echo "Error: go-tools module not found. Create it first."
    exit 1
fi

# Add all packages as build commands
for package in "$@"; do
    echo "Adding $package..."
    sprout-edit append-build go-tools "go install ${package}@latest"
done

echo "Added $# packages to go-tools"
echo "Run: sprout modules build go-tools --rebuild"
```

**Usage:**
```bash
sprout go-batch add \
    golang.org/x/tools/gopls \
    github.com/golangci/golangci-lint/cmd/golangci-lint \
    github.com/junegunn/fzf
```

**Result:**
```
module go-tools {
    build {
        env { ... }
        go install golang.org/x/tools/gopls@latest
        go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
        go install github.com/junegunn/fzf@latest
    }
}
```

### Strategy 4: Conditional Logic

**~/.config/sprout/commands/sprout-add**
```bash
#!/bin/bash
# Smart add - detects language and chooses strategy

set -e

PACKAGE=$1

if [ -z "$PACKAGE" ]; then
    echo "Usage: sprout add <package-url-or-name>"
    exit 1
fi

# Detect language
if echo "$PACKAGE" | grep -q "github.com.*\.git\|crates.io"; then
    # Rust
    CRATE=$(basename $PACKAGE .git)
    
    if sprout-edit exists rust-tools; then
        echo "Adding $CRATE to rust-tools..."
        sprout-edit append-build rust-tools "cargo install $CRATE"
    else
        echo "Creating standalone Rust module..."
        sprout rust add "$CRATE"
    fi
    
elif echo "$PACKAGE" | grep -q "github.com/.*/.*/\|golang.org"; then
    # Go
    if sprout-edit exists go-tools; then
        echo "Adding $PACKAGE to go-tools..."
        sprout-edit append-build go-tools "go install ${PACKAGE}@latest"
    else
        echo "Creating standalone Go module..."
        sprout go add "$PACKAGE"
    fi
else
    echo "Unknown package type: $PACKAGE"
    exit 1
fi
```

## Advanced Examples

### Query and Modify

**~/.config/sprout/commands/sprout-list-deps**
```bash
#!/bin/bash
# List all dependencies of a module

MODULE=$1

if [ -z "$MODULE" ]; then
    echo "Usage: sprout list-deps <module>"
    exit 1
fi

sprout-edit get "$MODULE" depends_on
```

### Conditional Exports

**~/.config/sprout/commands/sprout-add-lib-path**
```bash
#!/bin/bash
# Add library path if not already present

MODULE=$1
PATH=$2

# Get current exports
CURRENT=$(sprout-edit get "$MODULE" exports | grep LD_LIBRARY_PATH || true)

if echo "$CURRENT" | grep -q "$PATH"; then
    echo "Path already exists: $PATH"
else
    sprout-edit add-export "$MODULE" LD_LIBRARY_PATH "$PATH"
    echo "Added LD_LIBRARY_PATH=$PATH to $MODULE"
fi
```

### Migration Helper

**~/.config/sprout/commands/sprout-migrate-standalone**
```bash
#!/bin/bash
# Migrate standalone modules to a collection module

COLLECTION=$1
shift
MODULES=("$@")

if [ -z "$COLLECTION" ] || [ ${#MODULES[@]} -eq 0 ]; then
    echo "Usage: sprout migrate-standalone <collection> <module1> <module2> ..."
    exit 1
fi

# Create collection module if doesn't exist
if ! sprout-edit exists "$COLLECTION"; then
    echo "Creating $COLLECTION module..."
    sprout-edit add-module "$COLLECTION" \
        --build "echo 'Collection module'"
fi

# Move each module to depends_on
for module in "${MODULES[@]}"; do
    if sprout-edit exists "$module"; then
        echo "Adding $module to $COLLECTION..."
        sprout-edit append "$COLLECTION" depends_on "$module"
        # Optionally remove standalone module
        # sprout-edit remove-module "$module"
    fi
done

echo "Migration complete"
```

## Implementation Notes

### `sprout-edit` Implementation

The `sprout-edit` command:
1. Loads manifest with parser
2. Modifies AST in memory
3. Validates changes
4. Saves with pretty-print (preserves formatting)
5. Updates lockfile if needed

### Error Handling

Scripts should check exit codes:
```bash
if ! sprout-edit append go-tools depends_on gopls; then
    echo "Error: Failed to add gopls"
    exit 1
fi
```

### Atomic Operations

All `sprout-edit` operations are atomic:
- Load → Modify → Validate → Save
- If any step fails, manifest is unchanged

## Best Practices

1. **Check existence first**: Use `sprout-edit exists` before modifying
2. **Query before modify**: Use `sprout-edit get` to check current state
3. **Provide user choice**: Ask whether to extend or create new
4. **Validate inputs**: Check arguments before calling sprout-edit
5. **Support --help**: Document your script's usage
6. **Handle errors**: Check exit codes and provide helpful messages
7. **Use dry-run**: Support `--dry-run` for preview

## Comparison: Extend vs Standalone

### Extend Existing Module (e.g., go-tools)

**Pattern:** Append `go install` commands to build block

**Example:**
```
module go-tools {
    depends_on = [go]
    build {
        env { GOBIN = "${DIST_PATH}/bin" }
        go install github.com/jesseduffield/lazygit@v0.55.1
        go install golang.org/x/tools/gopls@latest
        go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest  # Added
    }
}
```

**Pros:**
- Single build command installs all tools
- Shared environment setup (GOBIN, PATH, etc.)
- Easier to update all at once
- Less manifest clutter
- All tools in one place

**Cons:**
- Rebuilds all tools when one changes
- All-or-nothing (can't selectively install one tool)
- Harder to track individual tool versions
- Longer build times

### Standalone Modules

**Pattern:** Separate module per tool

**Example:**
```
module gopls {
    depends_on = [go]
    fetch {
        git = { url = https://golang.org/x/tools }
    }
    build {
        go build -o ${DIST_PATH}/bin/gopls ./gopls
    }
    exports = { PATH = "/bin" }
}
```

**Pros:**
- Independent versioning per tool
- Selective installation
- Faster rebuilds (only changed tool)
- Clear dependency tracking
- Can pin specific versions

**Cons:**
- More manifest entries
- Duplicate dependencies possible
- More complex dependency graph
- Repeated boilerplate

### Recommendation
- **Extend (go-tools pattern)**: For related tools you always use together (e.g., Go dev tools, Rust CLI tools)
- **Standalone**: For independent tools, different versions needed, or tools with complex builds

## Future Enhancements

- [ ] `sprout-edit` with JSON output for easier parsing
- [ ] Transaction support (rollback on error)
- [ ] Diff preview before applying changes
- [ ] Template system for common patterns
- [ ] Script validation and linting
- [ ] Community script repository
