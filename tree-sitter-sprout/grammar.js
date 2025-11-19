module.exports = grammar({
  name: 'sprout',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  rules: {
    source_file: $ => repeat($._statement),

    _statement: $ => choice(
      $.module_block,
      $.environments_block,
    ),

    comment: $ => /#[^\n]*/,

    module_block: $ => seq(
      'module',
      $.identifier,
      '{',
      repeat($._module_field),
      '}'
    ),

    _module_field: $ => choice(
      $.depends_on_field,
      $.exports_field,
      $.fetch_block,
      $.build_block,
      $.install_block,
      $.update_block,
    ),

    depends_on_field: $ => seq(
      'depends_on',
      '=',
      $.array
    ),

    exports_field: $ => seq(
      'exports',
      '=',
      '{',
      repeat($.env_entry),
      '}'
    ),

    fetch_block: $ => seq(
      'fetch',
      '{',
      repeat(choice(
        $._fetch_spec,
        $.fetch_output_field
      )),
      '}'
    ),

    fetch_output_field: $ => seq(
      alias('output', $.field_name),
      '=',
      $.value
    ),

    _fetch_spec: $ => choice(
      $.git_spec,
      $.archive_spec,
      $.cargo_spec,
      $.go_spec,
      $.http_spec,
      $.local_spec,
    ),

    git_spec: $ => seq(
      'git',
      '=',
      '{',
      repeat($.git_field),
      '}'
    ),

    git_field: $ => choice(
      seq(alias('url', $.field_name), '=', $.value),
      seq(alias('ref', $.field_name), '=', $.value),
      seq(alias('depth', $.field_name), '=', $.number),
    ),

    archive_spec: $ => seq(
      'archive',
      '=',
      '{',
      repeat($.archive_field),
      '}'
    ),

    archive_field: $ => choice(
      seq(alias('url', $.field_name), '=', $.value),
      seq(alias('sha256', $.field_name), '=', $.value),
    ),

    cargo_spec: $ => seq(
      'cargo',
      '=',
      '{',
      repeat($.cargo_field),
      '}'
    ),

    cargo_field: $ => choice(
      seq(alias('crate', $.field_name), '=', $.value),
      seq(alias('version', $.field_name), '=', $.value),
    ),

    go_spec: $ => seq(
      'go',
      '=',
      '{',
      repeat($.go_field),
      '}'
    ),

    go_field: $ => choice(
      seq(alias('module', $.field_name), '=', $.value),
      seq(alias('version', $.field_name), '=', $.value),
    ),

    http_spec: $ => seq(
      'http',
      '=',
      '{',
      repeat($.http_field),
      '}'
    ),

    http_field: $ => choice(
      seq(alias('url', $.field_name), '=', $.value),
      seq(alias('sha256', $.field_name), '=', $.value),
    ),

    local_spec: $ => seq(
      'local',
      '=',
      '{',
      repeat($.local_field),
      '}'
    ),

    local_field: $ => seq(alias('path', $.field_name), '=', $.value),

    build_block: $ => seq(
      'build',
      '{',
      repeat(choice(
        $.env_block,
        $.command_line
      )),
      '}'
    ),

    install_block: $ => seq(
      'install',
      '{',
      repeat(choice(
        $.env_block,
        $.command_line
      )),
      '}'
    ),

    update_block: $ => seq(
      'update',
      '{',
      repeat(choice(
        $.env_block,
        $.command_line
      )),
      '}'
    ),

    env_block: $ => seq(
      'env',
      '{',
      repeat($.env_entry),
      '}'
    ),

    env_entry: $ => seq(
      $.identifier,
      '=',
      $.string
    ),

    command_line: $ => token(prec(-1, /[^\n]+/)),

    environments_block: $ => seq(
      'environments',
      '{',
      repeat($.environment_entry),
      '}'
    ),

    environment_entry: $ => seq(
      $.identifier,
      '=',
      $.array
    ),

    array: $ => seq(
      '[',
      optional(seq(
        $.value,
        repeat(seq(',', $.value)),
        optional(',')
      )),
      ']'
    ),

    map: $ => seq(
      '{',
      optional(seq(
        $.map_entry,
        repeat(seq(',', $.map_entry)),
        optional(',')
      )),
      '}'
    ),

    map_entry: $ => seq(
      $.identifier,
      '=',
      $.value
    ),

    value: $ => choice(
      $.string,
      $.unquoted_value
    ),

    string: $ => /"([^"\\]|\\.)*"/,

    unquoted_value: $ => /[^\s,{}\[\]]+/,

    number: $ => /\d+/,

    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_-]*/,

    field_name: $ => /[a-z_]+/,
  }
});
