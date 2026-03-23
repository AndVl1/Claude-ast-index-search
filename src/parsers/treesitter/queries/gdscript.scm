; GDScript tree-sitter queries for ast-index

; class_name statement: class_name MyClass
(class_name_statement
  name: (name) @class_name_decl)

; class definition: class InnerClass:
(class_definition
  name: (name) @class_def_name
  body: (class_body))

; extends statement: extends Node2D
(extends_statement
  (type) @extends_name)

; function definition: func my_func():
(function_definition
  name: (name) @func_name)

; signal: signal my_signal
(signal_statement
  name: (name) @signal_name)

; enum: enum State { IDLE, RUNNING }
(enum_definition
  name: (name) @enum_name)

; const: const MAX_SPEED = 100
(const_statement
  name: (name) @const_name)

; var: var speed = 10
(variable_statement
  name: (name) @var_name)

; export var: @export var speed: float = 10
(export_variable_statement
  name: (name) @export_var_name)

; @onready var: @onready var sprite = $Sprite
(onready_variable_statement
  name: (name) @onready_var_name)
