.PHONY: build run test clean bench doc check help

# Default target
all: build

# Build the project in release mode
build:
	@echo "Building B+ Tree Index in release mode..."
	cargo build --release

# Run the driver program
run: build
	@echo "Running driver program..."
	cargo run --release --bin driver

# Run tests
test:
	@echo "Running tests..."
	cargo test --release

# Clean build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -f bptree.idx
	rm -f *.idx

# Run benchmarks (requires nightly)
bench:
	@echo "Running benchmarks..."
	cargo +nightly bench

# Generate documentation
doc:
	@echo "Generating documentation..."
	cargo doc --no-deps --open

# Check code without building
check:
	@echo "Checking code..."
	cargo check

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt

# Run clippy lints
clippy:
	@echo "Running clippy..."
	cargo clippy -- -W clippy::all

# Install dependencies
deps:
	@echo "Installing dependencies..."
	cargo fetch

# Display help information
help:
	@echo "B+ Tree Index Makefile"
	@echo ""
	@echo "Available targets:"
	@echo "  build    - Build the project in release mode (optimized)"
	@echo "  run      - Build and run the driver program"
	@echo "  test     - Run all tests"
	@echo "  clean    - Remove build artifacts and index files"
	@echo "  bench    - Run benchmarks (requires nightly Rust)"
	@echo "  doc      - Generate and open documentation"
	@echo "  check    - Check code without building"
	@echo "  fmt      - Format code with rustfmt"
	@echo "  clippy   - Run clippy linter"
	@echo "  deps     - Download dependencies"
	@echo "  help     - Display this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make build       # Build the library and driver"
	@echo "  make run         # Run the test driver"
	@echo "  make test        # Run unit tests"
	@echo "  make clean run   # Clean and run"