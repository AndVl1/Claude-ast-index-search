; Classes
(class_declaration
  name: (identifier) @class_name) @class_node

; Records (index as class-like types)
(record_declaration
  name: (identifier) @class_name) @class_node

; Record components (header parameters in `record Foo(Type x, Type y)`)
(record_declaration
  parameters: (formal_parameters
    (formal_parameter
      name: (identifier) @record_component_name) @record_component_node))

; Interfaces
(interface_declaration
  name: (identifier) @interface_name) @interface_node

; Enums
(enum_declaration
  name: (identifier) @enum_name) @enum_node

; Methods
(method_declaration
  name: (identifier) @method_name) @method_node

; Constructors
(constructor_declaration
  name: (identifier) @constructor_name) @constructor_node

; Fields
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @field_name)) @field_node

; Annotations (marker - no arguments, like @Override)
(marker_annotation
  name: (identifier) @annotation_name)

; Annotations (with arguments, like @GetMapping("/users"))
(annotation
  name: (identifier) @annotation_call_name)
