; Class selector: .foo
(class_selector
  (class_name) @class_name)

; Id selector: #bar
(id_selector
  (id_name) @id_name)

; @keyframes name { ... }
(keyframes_statement
  (keyframes_name) @keyframes_name)

; @import "path";
(import_statement
  (string_value) @import_path)

; Custom property declaration: --my-var: value;
; Captures every declaration; rust filters by leading `--`.
(declaration
  (property_name) @custom_property)
