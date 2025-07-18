# Contributing to qml

We welcome contributions of all kinds! Whether you're fixing bugs, adding features, improving documentation, or enhancing tests, your help makes qml better for everyone.

## **Types of Contributions**

- 🐛 **Bug Reports**: Found an issue? Open an issue with details and reproduction steps
- ✨ **Feature Requests**: Have an idea? Discuss it in an issue first
- 📝 **Documentation**: Improve README, code comments, or add examples
- 🧪 **Testing**: Add tests for edge cases or improve test coverage
- 🚀 **Performance**: Optimize code or add benchmarks
- 🔧 **Code**: Fix bugs, implement features, or refactor code

## **Development Setup**

1. **Fork and Clone**:

```bash
git clone https://github.com/yourusername/qml.git
cd qml
```

1. **Install Dependencies**:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build the project
cargo build

# Run basic tests
cargo test
```

1. **Setup Development Environment**:

```bash
# Install useful development tools
cargo install cargo-watch cargo-audit

# Run tests in watch mode during development
cargo watch -x test
```

## **Testing with Backends**

For comprehensive testing, you'll need PostgreSQL and Redis:

```bash
# Start PostgreSQL
docker run -d --name qml-postgres \
  -e POSTGRES_PASSWORD=test \
  -e POSTGRES_DB=qml_test \
  -p 5432:5432 postgres:15

# Start Redis
docker run -d --name qml-redis \
  -p 6379:6379 redis:7-alpine

# Run all tests including backend tests
cargo test --features postgres

# Run specific test suites
cargo test storage       # Storage tests
cargo test test_locking  # Race condition tests
cargo test integration   # Integration tests
```

## **Code Style & Guidelines**

- **Formatting**: Use `cargo fmt` before committing
- **Linting**: Run `cargo clippy` and fix warnings
- **Documentation**: Add doc comments for public APIs
- **Testing**: Add tests for new functionality
- **Error Handling**: Use proper error types from `src/error.rs`
- **Async**: Use `async`/`await` consistently

```bash
# Run quality checks
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo doc --no-deps
```

## **Pull Request Process**

1. **Create a Branch**:

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/bug-description
```

1. **Make Changes**:
   - Write clean, well-documented code
   - Add tests for new functionality
   - Update documentation if needed
   - Ensure all tests pass

1. **Quality Checks**:

```bash
# Run the full test suite
cargo test --all-features

# Check formatting and linting
cargo fmt --check
cargo clippy -- -D warnings

# Verify documentation builds
cargo doc --no-deps
```

1. **Commit and Push**:

```bash
git add .
git commit -m "feat: add new feature" # or "fix:", "docs:", etc.
git push origin feature/your-feature-name
```

1. **Open Pull Request**:
   - Describe what your PR does
   - Reference any related issues
   - Include testing instructions
   - Wait for review and address feedback

## **Commit Message Format**

We use conventional commits:

- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `test:` - Adding or updating tests
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

Examples:

```text
feat: add Redis connection pooling
fix: resolve race condition in PostgreSQL locking
docs: update README with new examples
test: add stress tests for worker pools
```

## **Getting Help**

- 💬 **Questions**: Open an issue with the "question" label
- 🐛 **Bugs**: Use the bug report template
- 💡 **Features**: Discuss in an issue before implementing
- 📧 **Contact**: Reach out to maintainers for complex contributions

## **Development Environment Variables**

Create a `.env` file for local development:

```bash
# Database URLs for testing
DATABASE_URL=postgresql://postgres:test@localhost:5432/qml_test
REDIS_URL=redis://localhost:6379

# Logging
RUST_LOG=debug

# Test configuration
QML_TEST_POSTGRES=true
QML_TEST_REDIS=true
```

## **Project Structure**

Understanding the codebase structure will help you navigate and contribute effectively:

```text
qml/
├── src/
│   ├── lib.rs              # Main library entry point
│   ├── error.rs            # Error types and handling
│   ├── core/               # Core job and state definitions
│   │   ├── job.rs          # Job struct and methods
│   │   ├── job_state.rs    # Job state enum and transitions
│   │   └── mod.rs
│   ├── storage/            # Storage backend implementations
│   │   ├── memory.rs       # In-memory storage
│   │   ├── postgres.rs     # PostgreSQL storage
│   │   ├── redis.rs        # Redis storage
│   │   └── mod.rs
│   ├── processing/         # Job processing engine
│   │   ├── worker.rs       # Worker trait and implementation
│   │   ├── scheduler.rs    # Job scheduling logic
│   │   ├── processor.rs    # Main processing engine
│   │   └── mod.rs
│   └── dashboard/          # Web dashboard
│       ├── server.rs       # HTTP server
│       ├── routes.rs       # API routes
│       ├── websocket.rs    # WebSocket handlers
│       └── mod.rs
├── examples/               # Usage examples
├── tests/                  # Integration tests
├── migrations/             # Database migrations
└── README.md
```

## **Testing Guidelines**

### **Test Categories**

1. **Unit Tests**: Test individual functions and methods
2. **Integration Tests**: Test component interactions
3. **Backend Tests**: Test storage backend implementations
4. **Race Condition Tests**: Test concurrent access scenarios
5. **Stress Tests**: Test high-load scenarios

### **Writing Tests**

- Place unit tests in the same file as the code being tested
- Place integration tests in the `tests/` directory
- Use descriptive test names that explain what is being tested
- Include both positive and negative test cases
- Test error conditions and edge cases

### **Running Specific Tests**

```bash
# Run tests for a specific module
cargo test storage

# Run a specific test
cargo test test_job_creation

# Run tests with output
cargo test -- --nocapture

# Run tests in parallel
cargo test -- --test-threads=4
```

## **Documentation Guidelines**

### **Code Documentation**

- Add doc comments (`///`) for all public APIs
- Include examples in doc comments where helpful
- Document function parameters and return values
- Explain complex algorithms or business logic

### **README Updates**

- Keep examples up-to-date with code changes
- Update feature lists when adding new functionality
- Ensure installation instructions remain accurate
- Add new examples to the examples section

## **Performance Considerations**

When contributing performance improvements:

- Include benchmarks to demonstrate improvements
- Test with realistic data sizes and concurrency levels
- Consider memory usage as well as speed
- Document any trade-offs made

## **Security Guidelines**

- Never commit secrets or credentials
- Use environment variables for configuration
- Validate all inputs from external sources
- Follow Rust security best practices
- Report security issues privately to maintainers

## **Release Process**

For maintainers preparing releases:

1. Update version numbers in `Cargo.toml`
2. Update CHANGELOG.md with release notes
3. Run full test suite across all backends
4. Create release tag and GitHub release
5. Publish to crates.io

Thank you for contributing to qml! Your efforts help make background job processing in Rust better for everyone.
