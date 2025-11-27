# Makefile for B+ Tree Database Index (Rust)
# Author: Assignment Implementation
# Date: November 2025

# Variables
CARGO = cargo
TARGET = bptree_test
RELEASE_DIR = target/release
DEBUG_DIR = target/debug
INDEX_FILE = bptree_index.dat
LIB_NAME = libbptree.so

# Default target
.PHONY: all
all: release
	@echo "========================================="
	@echo "Build completed successfully!"
	@echo "Run 'make run' to execute tests"
	@echo "========================================="

# Build release version (optimized)
.PHONY: release
release:
	@echo "Building release version with optimizations..."
	$(CARGO) build --release
	@echo "✓ Release build complete: $(RELEASE_DIR)/$(TARGET)"

# Build debug version
.PHONY: debug
debug:
	@echo "Building debug version..."
	$(CARGO) build
	@echo "✓ Debug build complete: $(DEBUG_DIR)/$(TARGET)"

# Build library only
.PHONY: lib
lib:
	@echo "Building library..."
	$(CARGO) build --release --lib
	@echo "✓ Library build complete: $(RELEASE_DIR)/$(LIB_NAME)"

# Run tests with release build
.PHONY: run
run: release
	@echo "========================================="
	@echo "Running test suite (release mode)..."
	@echo "========================================="
	$(CARGO) run --release

# Run tests with debug build
.PHONY: run-debug
run-debug: debug
	@echo "========================================="
	@echo "Running test suite (debug mode)..."
	@echo "========================================="
	$(CARGO) run

# Run unit tests
.PHONY: test
test:
	@echo "Running unit tests..."
	$(CARGO) test

# Run tests with output
.PHONY: test-verbose
test-verbose:
	@echo "Running unit tests (verbose)..."
	$(CARGO) test -- --nocapture

# Check code without building
.PHONY: check
check:
	@echo "Checking code..."
	$(CARGO) check

# Format code
.PHONY: fmt
fmt:
	@echo "Formatting code..."
	$(CARGO) fmt

# Check formatting
.PHONY: fmt-check
fmt-check:
	@echo "Checking code formatting..."
	$(CARGO) fmt -- --check

# Run clippy (linter)
.PHONY: lint
lint:
	@echo "Running Clippy linter..."
	$(CARGO) clippy -- -D warnings

# Run all quality checks
.PHONY: quality
quality: fmt-check lint test
	@echo "✓ All quality checks passed!"

# Clean build artifacts
.PHONY: clean
clean:
	@echo "Cleaning build artifacts..."
	$(CARGO) clean
	@echo "✓ Clean complete"

# Clean everything including index file
.PHONY: distclean
distclean: clean
	@echo "Removing index file..."
	rm -f $(INDEX_FILE)
	@echo "✓ Complete clean finished"

# Build documentation
.PHONY: doc
doc:
	@echo "Building documentation..."
	$(CARGO) doc --no-deps

# Build and open documentation
.PHONY: doc-open
doc-open:
	@echo "Building and opening documentation..."
	$(CARGO) doc --no-deps --open

# Benchmark
.PHONY: bench
bench: release
	@echo "Running benchmarks..."
	@time $(RELEASE_DIR)/$(TARGET)

# Profile with perf (Linux only)
.PHONY: profile
profile: release
	@echo "Profiling with perf..."
	perf record -g $(RELEASE_DIR)/$(TARGET)
	perf report

# Memory check with valgrind
.PHONY: memcheck
memcheck: release
	@echo "Running memory leak detection..."
	valgrind --leak-check=full --show-leak-kinds=all $(RELEASE_DIR)/$(TARGET)

# Install Rust (if needed)
.PHONY: install-rust
install-rust:
	@echo "Installing Rust..."
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
	@echo "Run 'source $$HOME/.cargo/env' to configure your shell"

# Update Rust toolchain
.PHONY: update-rust
update-rust:
	@echo "Updating Rust toolchain..."
	rustup update

# Install system-wide (requires sudo)
.PHONY: install
install: release
	@echo "Installing to /usr/local/bin..."
	sudo cp $(RELEASE_DIR)/$(TARGET) /usr/local/bin/
	@echo "✓ Installation complete"

# Uninstall from system
.PHONY: uninstall
uninstall:
	@echo "Uninstalling from /usr/local/bin..."
	sudo rm -f /usr/local/bin/$(TARGET)
	@echo "✓ Uninstall complete"

# Create distribution package
.PHONY: dist
dist: release
	@echo "Creating distribution package..."
	mkdir -p dist
	cp $(RELEASE_DIR)/$(TARGET) dist/
	cp $(RELEASE_DIR)/$(LIB_NAME) dist/ 2>/dev/null || true
	cp README.md dist/
	tar czf bptree_rust_dist.tar.gz dist/
	rm -rf dist
	@echo "✓ Distribution package: bptree_rust_dist.tar.gz"

# Watch for changes and rebuild
.PHONY: watch
watch:
	@echo "Watching for changes..."
	$(CARGO) watch -x build

# Show size of binary
.PHONY: size
size: release
	@echo "Binary size analysis:"
	@ls -lh $(RELEASE_DIR)/$(TARGET)
	@size $(RELEASE_DIR)/$(TARGET)

# Assembly output (for optimization analysis)
.PHONY: asm
asm:
	@echo "Generating assembly output..."
	$(CARGO) rustc --release -- --emit asm
	@echo "Assembly files in target/release/deps/"

# Dependency tree
.PHONY: deps
deps:
	@echo "Dependency tree:"
	$(CARGO) tree

# Update dependencies
.PHONY: update-deps
update-deps:
	@echo "Updating dependencies..."
	$(CARGO) update

# Security audit
.PHONY: audit
audit:
	@echo "Running security audit..."
	$(CARGO) audit || echo "Install with: cargo install cargo-audit"

# Coverage report (requires tarpaulin)
.PHONY: coverage
coverage:
	@echo "Generating coverage report..."
	$(CARGO) tarpaulin --out Html --output-dir coverage || \
		echo "Install with: cargo install cargo-tarpaulin"

# Compare with C++ version
.PHONY: compare
compare: release
	@echo "Performance comparison with C++ version:"
	@echo "Building C++ version..."
	@g++ -O3 -std=c++11 bptree.cpp driver.cpp -o bptree_cpp 2>/dev/null || \
		echo "C++ version not found"
	@echo "\nRust version:"
	@time $(RELEASE_DIR)/$(TARGET) > /dev/null
	@echo "\nC++ version:"
	@time ./bptree_cpp > /dev/null 2>&1 || echo "C++ binary not available"

# Quick build and run (for development)
.PHONY: quick
quick:
	$(CARGO) run

# Help target
.PHONY: help
help:
	@echo "B+ Tree Database Index (Rust) - Makefile Help"
	@echo "=============================================="
	@echo ""
	@echo "Build targets:"
	@echo "  make           - Build release version (default)"
	@echo "  make release   - Build optimized release version"
	@echo "  make debug     - Build debug version"
	@echo "  make lib       - Build library only"
	@echo "  make check     - Check code without building"
	@echo ""
	@echo "Run targets:"
	@echo "  make run       - Build and run (release)"
	@echo "  make run-debug - Build and run (debug)"
	@echo "  make quick     - Quick build and run"
	@echo "  make test      - Run unit tests"
	@echo "  make bench     - Run benchmarks"
	@echo ""
	@echo "Quality targets:"
	@echo "  make fmt       - Format code"
	@echo "  make lint      - Run linter (clippy)"
	@echo "  make quality   - Run all quality checks"
	@echo "  make audit     - Security audit"
	@echo "  make coverage  - Generate coverage report"
	@echo ""
	@echo "Clean targets:"
	@echo "  make clean     - Remove build artifacts"
	@echo "  make distclean - Remove all generated files"
	@echo ""
	@echo "Documentation:"
	@echo "  make doc       - Build documentation"
	@echo "  make doc-open  - Build and open documentation"
	@echo ""
	@echo "Analysis:"
	@echo "  make size      - Show binary size"
	@echo "  make deps      - Show dependency tree"
	@echo "  make profile   - Profile with perf"
	@echo "  make memcheck  - Check memory leaks"
	@echo ""
	@echo "Installation:"
	@echo "  make install-rust   - Install Rust toolchain"
	@echo "  make install        - Install to system"
	@echo "  make uninstall      - Remove from system"
	@echo "  make dist           - Create distribution package"
	@echo ""
	@echo "Examples:"
	@echo "  make && make run"
	@echo "  make clean && make release"
	@echo "  make quality && make run"

# Version info
.PHONY: version
version:
	@echo "Rust version:"
	@rustc --version
	@echo "\nCargo version:"
	@cargo --version
	@echo "\nProject version:"
	@grep "^version" Cargo.toml

# CI target (for continuous integration)
.PHONY: ci
ci: fmt-check lint test release
	@echo "✓ CI pipeline passed!"

# Default shell
SHELL := /bin/bash

# Prevent built-in rules
.SUFFIXES: