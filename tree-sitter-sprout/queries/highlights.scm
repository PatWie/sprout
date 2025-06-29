; Keywords
[
  "module"
  "depends_on"
  "exports"
  "fetch"
  "build"
  "install"
  "update"
  "env"
  "environments"
] @keyword

; Fetch types
[
  "git"
  "http"
  "local"
] @keyword

; Module name
(module_block 
  (identifier) @namespace)

; Module names in depends_on arrays (unquoted)
(depends_on_field (array (value (unquoted_value) @namespace)))

; Module names in depends_on arrays (quoted)
(depends_on_field (array (value (string) @namespace)))

; Environment variables in env blocks
(env_entry (identifier) @constant)

; Strings
(string) @string

; Comments
(comment) @comment

; Numbers
(number) @number

; Operators
["=" "{" "}" "[" "]" ","] @operator

; Command lines
(command_line) @text

; HTTP field values (always strings)
(http_field (value) @string)

; Git field values (always strings)
(git_field (value) @string)

; Environment variable values (always strings)
(env_entry (string) @string)

; Field names (keyword style) - must be last for priority
(field_name) @keyword

