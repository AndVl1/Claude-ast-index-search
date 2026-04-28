; CSS-inherited captures
(class_selector
  (class_name) @class_name)

(id_selector
  (id_name) @id_name)

(keyframes_statement
  (keyframes_name) @keyframes_name)

(import_statement
  (string_value) @import_path)

; Less-specific: `.mixin(args) { ... }`
;   `_mixin_name` is internal; its child class_name carries the bare
;   identifier. We capture it under a distinct name so the rust side knows
;   it's a mixin definition, not a regular class selector.
(mixin_definition
  (class_name) @less_mixin_def)

; Less variable declarations: `@brand: #ff0;`
; Declaration LHS is aliased to property_name; rust filters by leading `@`.
(declaration
  (property_name) @less_variable)
