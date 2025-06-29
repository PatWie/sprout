; Inject bash syntax into command lines
((command_line) @injection.content
 (#set! injection.language "bash"))

; Inject bash syntax into env entries (variable assignments)
((env_entry) @injection.content
 (#set! injection.language "bash"))
