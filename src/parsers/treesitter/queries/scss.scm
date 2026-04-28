; CSS-inherited captures
(class_selector
  (class_name) @class_name)

(id_selector
  (id_name) @id_name)

(keyframes_statement
  (keyframes_name) @keyframes_name)

(import_statement
  (string_value) @import_path)

; SCSS-specific
(use_statement
  (string_value) @use_path)

(forward_statement
  (string_value) @forward_path)

(mixin_statement
  name: (identifier) @mixin_name)

(function_statement
  name: (identifier) @function_name)

(placeholder
  (identifier) @placeholder_name)

; In SCSS the declaration LHS is aliased to property_name and may hold a
; variable (`$foo`), an identifier (`color`) or a custom property (`--foo`).
; Rust filters by leading `$` (variable) or `--` (custom property).
(declaration
  (property_name) @scss_variable)
