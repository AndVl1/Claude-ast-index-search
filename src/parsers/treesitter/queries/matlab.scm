; Class definition
(class_definition
  name: (identifier) @class_name)

; Function definition (identifier form)
(function_definition
  name: (identifier) @func_name)

; Function definition (property_name form, e.g., set.Prop / get.Prop)
(function_definition
  name: (property_name) @func_name)

; Properties block with property declarations
(property
  name: (identifier) @property_name)

; Enumeration members
(enum
  (identifier) @enum_name)

; Events
(events
  (identifier) @event_name)
