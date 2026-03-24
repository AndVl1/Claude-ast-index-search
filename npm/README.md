# @ast-index/cli

Fast AST-based code search CLI for 30 programming languages. Native Rust binary distributed via npm.

## Install

```bash
npm install -g @ast-index/cli
```

Or use directly:

```bash
npx @ast-index/cli rebuild
npx @ast-index/cli search MyClass
```

## Supported Languages

Kotlin, Java, Swift, Objective-C, TypeScript, JavaScript, Vue, Svelte, Rust, Ruby, C#, Python, Go, C++, Scala, PHP, Dart, Perl, Lua, Elixir, Bash, SQL, R, Matlab, Groovy, Common Lisp, GDScript, BSL, Protocol Buffers, WSDL/XSD.

## Usage

```bash
# Index your project
ast-index rebuild

# Search symbols
ast-index search MyClass
ast-index symbol MyFunction
ast-index class UserService

# Find usages and callers
ast-index usages MyMethod
ast-index callers handleClick

# Class hierarchy
ast-index hierarchy BaseController
ast-index implementations Animal
```

## Platforms

| Platform | Package |
|----------|---------|
| macOS ARM64 | `@ast-index/cli-darwin-arm64` |
| macOS x64 | `@ast-index/cli-darwin-x64` |
| Linux x64 | `@ast-index/cli-linux-x64` |
| Linux ARM64 | `@ast-index/cli-linux-arm64` |
| Windows x64 | `@ast-index/cli-win32-x64` |

## Also available via

- **Homebrew**: `brew install defendend/ast-index/ast-index`
- **GitHub Releases**: [download binaries](https://github.com/defendend/Claude-ast-index-search/releases)

## License

MIT
