fn main() {
    // Compile tree-sitter-bsl C parser
    cc::Build::new()
        .include("tree-sitter-bsl/src")
        .file("tree-sitter-bsl/src/parser.c")
        .std("c11")
        .warnings(false)
        .compile("tree-sitter-bsl");
}
