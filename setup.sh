#!/bin/bash
# B+ Tree Rust Setup Script
# Automatically sets up the project structure

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

PROJECT_NAME="bptree_rust"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  B+ Tree Rust Project Setup${NC}"
echo -e "${BLUE}========================================${NC}\n"

# Check if Rust is installed
check_rust() {
    echo -e "${YELLOW}Checking for Rust installation...${NC}"
    if command -v rustc &> /dev/null; then
        RUST_VERSION=$(rustc --version)
        echo -e "${GREEN}✓ Rust found: $RUST_VERSION${NC}"
        return 0
    else
        echo -e "${RED}✗ Rust not found${NC}"
        return 1
    fi
}

# Install Rust
install_rust() {
    echo -e "${YELLOW}Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo -e "${GREEN}✓ Rust installed successfully${NC}"
}

# Create project structure
create_project() {
    echo -e "${YELLOW}Creating project structure...${NC}"
    
    if [ -d "$PROJECT_NAME" ]; then
        echo -e "${RED}✗ Directory $PROJECT_NAME already exists${NC}"
        read -p "Remove and recreate? (y/n) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            rm -rf "$PROJECT_NAME"
        else
            echo -e "${RED}Aborting.${NC}"
            exit 1
        fi
    fi
    
    cargo new "$PROJECT_NAME" --lib
    cd "$PROJECT_NAME"
    echo -e "${GREEN}✓ Project created: $PROJECT_NAME${NC}"
}

# Create Cargo.toml
create_cargo_toml() {
    echo -e "${YELLOW}Creating Cargo.toml...${NC}"
    cat > Cargo.toml << 'EOF'
[package]
name = "bptree_rust"
version = "1.0.0"
edition = "2021"
authors = ["Assignment Implementation"]

[lib]
name = "bptree"
crate-type = ["cdylib", "rlib"]

[[bin]]
name = "bptree_test"
path = "src/main.rs"

[dependencies]
memmap2 = "0.9"
lazy_static = "1.4"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true

[profile.dev]
opt-level = 0

[profile.bench]
opt-level = 3
EOF
    echo -e "${GREEN}✓ Cargo.toml created${NC}"
}

# Create Makefile
create_makefile() {
    echo -e "${YELLOW}Creating Makefile...${NC}"
    cat > Makefile << 'EOF'
# Simplified Makefile for B+ Tree Rust

.PHONY: all build run test clean help

all: build

build:
	@cargo build --release

run:
	@cargo run --release

test:
	@cargo test

clean:
	@cargo clean
	@rm -f bptree_index.dat

help:
	@echo "Available targets:"
	@echo "  make build  - Build release version"
	@echo "  make run    - Build and run tests"
	@echo "  make test   - Run unit tests"
	@echo "  make clean  - Clean build artifacts"
EOF
    echo -e "${GREEN}✓ Makefile created${NC}"
}

# Create README
create_readme() {
    echo -e "${YELLOW}Creating README.md...${NC}"
    cat > README.md << 'EOF'
# B+ Tree Database Index - Rust Implementation

## Quick Start

```bash
# Build
make build
# or
cargo build --release

# Run
make run
# or
cargo run --release

# Test
make test
# or
cargo test
```

## Requirements

- Rust 1.70 or later
- Linux/Ubuntu

## Features

- High-performance B+ tree implementation
- Memory-mapped I/O for efficient disk access
- Complete test suite with benchmarks
- C-compatible FFI for interoperability

See MIGRATION_GUIDE.md for detailed documentation.
EOF
    echo -e "${GREEN}✓ README.md created${NC}"
}

# Create .gitignore
create_gitignore() {
    echo -e "${YELLOW}Creating .gitignore...${NC}"
    cat > .gitignore << 'EOF'
# Rust
/target/
Cargo.lock
**/*.rs.bk

# Index file
bptree_index.dat

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db
EOF
    echo -e "${GREEN}✓ .gitignore created${NC}"
}

# Show next steps
show_next_steps() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${GREEN}✓ Setup Complete!${NC}"
    echo -e "${BLUE}========================================${NC}\n"
    
    echo -e "${YELLOW}Next steps:${NC}\n"
    echo -e "1. Navigate to project directory:"
    echo -e "   ${BLUE}cd $PROJECT_NAME${NC}\n"
    
    echo -e "2. Copy the source files:"
    echo -e "   ${BLUE}# Copy lib.rs content to src/lib.rs${NC}"
    echo -e "   ${BLUE}# Copy main.rs content to src/main.rs${NC}\n"
    
    echo -e "3. Build the project:"
    echo -e "   ${BLUE}make build${NC}"
    echo -e "   or"
    echo -e "   ${BLUE}cargo build --release${NC}\n"
    
    echo -e "4. Run tests:"
    echo -e "   ${BLUE}make run${NC}"
    echo -e "   or"
    echo -e "   ${BLUE}cargo run --release${NC}\n"
    
    echo -e "${YELLOW}Additional commands:${NC}\n"
    echo -e "  ${BLUE}make test${NC}   - Run unit tests"
    echo -e "  ${BLUE}make clean${NC}  - Clean build artifacts"
    echo -e "  ${BLUE}cargo doc --open${NC}  - Open documentation\n"
    
    echo -e "${GREEN}Happy coding!${NC}\n"
}

# Main execution
main() {
    if ! check_rust; then
        echo -e "${YELLOW}Rust is required but not installed.${NC}"
        read -p "Install Rust now? (y/n) " -n 1 -r
        echo
        if [[ $REPLY =~ ^[Yy]$ ]]; then
            install_rust
            source "$HOME/.cargo/env"
        else
            echo -e "${RED}Cannot continue without Rust. Exiting.${NC}"
            exit 1
        fi
    fi
    
    create_project
    create_cargo_toml
    create_makefile
    create_readme
    create_gitignore
    
    # Create src directory structure
    mkdir -p src
    touch src/lib.rs
    touch src/main.rs
    
    show_next_steps
}

# Run main function
main