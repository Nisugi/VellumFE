# Contributing Guide

How to contribute to VellumFE development.

## Welcome!

Thank you for considering contributing to VellumFE! This document provides guidelines and information for contributors.

## Ways to Contribute

### Code Contributions

- Bug fixes
- New features
- Performance improvements
- Widget implementations
- Parser extensions

### Non-Code Contributions

- Documentation improvements
- Bug reports
- Feature suggestions
- User support
- Testing and feedback

## Getting Started

### 1. Fork and Clone

```bash
# Fork on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/vellum-fe.git
cd vellum-fe
```

### 2. Set Up Development Environment

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install dependencies (varies by platform)
# See building.md for platform-specific instructions

# Build
cargo build

# Run tests
cargo test
```

### 3. Create a Branch

```bash
git checkout -b feature/my-feature
# or
git checkout -b fix/bug-description
```

## Development Workflow

### Making Changes

1. **Write code** following the style guide
2. **Add tests** for new functionality
3. **Update documentation** if needed
4. **Run checks** before committing

### Pre-Commit Checks

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Run tests
cargo test

# Check documentation
cargo doc --no-deps
```

### Commit Messages

Follow conventional commit format:

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `refactor`: Code restructuring
- `test`: Adding tests
- `chore`: Maintenance

Examples:
```
feat(parser): add spell duration parsing

fix(widget): correct scrollback overflow

docs: update installation instructions

refactor(tui): extract render helpers
```

## Pull Request Process

### Before Submitting

- [ ] Code follows style guide
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Documentation updated
- [ ] Commit messages are clear

### PR Description Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
How was this tested?

## Checklist
- [ ] Tests added
- [ ] Documentation updated
- [ ] No breaking changes (or documented)
```

### Review Process

1. Submit PR against `main` branch
2. CI checks run automatically
3. Maintainers review code
4. Address feedback
5. Merge when approved

## Code Style

### Rust Style

Follow standard Rust conventions:

```rust
// Use rustfmt defaults
cargo fmt

// Naming
fn function_name() {}       // snake_case for functions
struct StructName {}        // PascalCase for types
const CONSTANT: u32 = 1;    // SCREAMING_SNAKE_CASE for constants
let variable_name = 1;      // snake_case for variables
```

### Documentation

```rust
/// Brief description of the function.
///
/// More detailed explanation if needed.
///
/// # Arguments
///
/// * `param` - Description of parameter
///
/// # Returns
///
/// Description of return value
///
/// # Examples
///
/// ```
/// let result = my_function("input");
/// assert_eq!(result, expected);
/// ```
pub fn my_function(param: &str) -> Result<Output> {
    // ...
}
```

### Error Handling

```rust
// Use Result for recoverable errors
fn parse_config(path: &Path) -> Result<Config, ConfigError> {
    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

// Use Option for optional values
fn find_widget(&self, name: &str) -> Option<&Widget> {
    self.widgets.iter().find(|w| w.name() == name)
}
```

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptive_name() {
        // Arrange
        let input = setup_test_data();

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

## Issue Guidelines

### Bug Reports

Include:
- VellumFE version
- Operating system
- Steps to reproduce
- Expected behavior
- Actual behavior
- Relevant logs

Template:
```markdown
**Version**: 0.1.0
**OS**: Windows 10 / macOS 14 / Ubuntu 22.04

**Steps to Reproduce**:
1. Do this
2. Then this
3. See error

**Expected**: What should happen
**Actual**: What actually happens

**Logs**:
```
relevant log output
```
```

### Feature Requests

Include:
- Use case description
- Proposed solution
- Alternatives considered

Template:
```markdown
**Use Case**:
Why do you need this feature?

**Proposed Solution**:
How should it work?

**Alternatives**:
What other approaches did you consider?
```

## Community Guidelines

### Code of Conduct

- Be respectful and inclusive
- Focus on constructive feedback
- Help others learn
- Credit others' work
- Assume good intentions

### Communication

- GitHub Issues for bugs and features
- GitHub Discussions for questions
- Pull Request comments for code review

### Getting Help

- Check existing documentation
- Search closed issues
- Ask in GitHub Discussions
- Be specific in your questions

## Release Process

### Version Numbering

Follow Semantic Versioning (SemVer):
- MAJOR: Breaking changes
- MINOR: New features (backwards compatible)
- PATCH: Bug fixes

### Changelog

Update `CHANGELOG.md` with:
- Version number and date
- Breaking changes (if any)
- New features
- Bug fixes
- Known issues

## Recognition

Contributors are recognized in:
- `CONTRIBUTORS.md`
- Release notes
- Project README

Thank you for contributing to VellumFE!

## See Also

- [Building](./building.md) - Build instructions
- [Project Structure](./project-structure.md) - Code organization
- [Testing](./testing.md) - Test guidelines

