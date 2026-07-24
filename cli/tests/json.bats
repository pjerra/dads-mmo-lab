#!/usr/bin/env bats
# Contract tests for the JSON emit helpers.

setup() {
  source "$BATS_TEST_DIRNAME/../src/10-json.sh"
}

@test "json_ok wraps data in success envelope" {
  run json_ok '{"version":"3.0.0"}'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}

@test "json_ok defaults data to null" {
  run json_ok
  [ "$(echo "$output" | jq -c '.data')" = "null" ]
}

@test "json_err builds error envelope with code/message/hint" {
  run json_err DOCKER_DOWN 'Docker is not running' 'Try: sudo systemctl start docker'
  [ "$(echo "$output" | jq -r '.ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_DOWN" ]
  [ "$(echo "$output" | jq -r '.error.message')" = "Docker is not running" ]
  [ "$(echo "$output" | jq -r '.error.hint')" = "Try: sudo systemctl start docker" ]
}

@test "json_escape handles quotes backslashes and newlines" {
  run json_escape $'he said "hi\\" and\nleft'
  [ "$output" = 'he said \"hi\\\" and\nleft' ]
}

@test "json_escape converts carriage return to literal \\r" {
  run json_escape $'line one\rline two'
  [ "$output" = 'line one\rline two' ]
}

@test "json_escape converts tab to literal \\t" {
  run json_escape $'col1\tcol2'
  [ "$output" = 'col1\tcol2' ]
}

@test "json_escape strips raw control bytes not covered by named escapes" {
  run json_escape $'before'$'\x01''after'
  [ "$output" = 'beforeafter' ]
}

@test "json_escape_var matches json_escape byte-for-byte on a torture string" {
  local s=$'AB\\C"D\tE\nF\rG<>&héllo\x01\x07\x0b\x0c\x1b\x1fend'
  json_escape_var "$s"
  [ "$REPLY" = "$(json_escape "$s")" ]
}

@test "json_escape_var: backslash and quote runs match json_escape" {
  local s='a\b"c\\d""'
  json_escape_var "$s"
  [ "$REPLY" = "$(json_escape "$s")" ]
}

@test "json_escape_var: control bytes (escaped and stripped) match json_escape" {
  local s=$'\x01\x02\x08\x09\x0a\x0b\x0c\x0d\x0e\x1f'
  json_escape_var "$s"
  [ "$REPLY" = "$(json_escape "$s")" ]
}

@test "json_escape_var: unicode and <>& pass through like json_escape" {
  local s='héllo <b>&amp; ünîcödé ✓'
  json_escape_var "$s"
  [ "$REPLY" = "$(json_escape "$s")" ]
}

@test "json_escape_var: empty and trailing-backslash inputs match json_escape" {
  json_escape_var ""
  [ "$REPLY" = "$(json_escape "")" ]
  json_escape_var 'ends with a backslash\'
  [ "$REPLY" = "$(json_escape 'ends with a backslash\')" ]
}

@test "ndjson_line emits a single valid JSON line" {
  run ndjson_line info 'Starting wow...'
  [ "$(echo "$output" | jq -r '.event')" = "line" ]
  [ "$(echo "$output" | jq -r '.level')" = "info" ]
  [ "$(echo "$output" | jq -r '.text')" = "Starting wow..." ]
}

@test "ndjson section and done events are valid JSON" {
  run ndjson_section_start start
  [ "$(echo "$output" | jq -r '.event')" = "section_start" ]
  run ndjson_section_end start ok
  [ "$(echo "$output" | jq -r '.status')" = "ok" ]
  run ndjson_done '{"state":"running"}'
  [ "$(echo "$output" | jq -r '.event')" = "done" ]
  [ "$(echo "$output" | jq -r '.data.state')" = "running" ]
  run ndjson_error NOT_FOUND 'no such title' ''
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
