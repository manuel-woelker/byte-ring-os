#!/usr/bin/env bash
# Format the index version of Markdown without staging unrelated working-copy edits.
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
cd -- "${repository_root}"
temporary_directory="$(mktemp -d)"
trap 'rm -rf -- "${temporary_directory}"' EXIT

git diff --cached --name-only --diff-filter=ACMR -z > "${temporary_directory}/paths"
paths=()
outputs=()
update_working=()

while IFS= read -r -d '' path; do
  [[ "${path}" == *.md ]] || continue

  git --literal-pathspecs ls-files --stage -z -- "${path}" > "${temporary_directory}/entry"
  IFS= read -r -d '' entry < "${temporary_directory}/entry"
  read -r mode object_id stage <<< "${entry%%$'\t'*}"
  if [[ "${stage}" != 0 || ( "${mode}" != 100644 && "${mode}" != 100755 ) ]]; then
    printf 'Expected a staged regular file: %s\n' "${path}" >&2
    exit 1
  fi

  number="${#paths[@]}"
  source="${temporary_directory}/${number}.md"
  output="${temporary_directory}/${number}.formatted.md"
  git cat-file blob "${object_id}" > "${source}"
  printf 'Formatting staged %s\n' "${path}"
  ./t rumdl fmt --stdin-filename "${path}" --silent - < "${source}" > "${output}"

  new_object_id="$(git hash-object -w -- "${output}")"
  printf '%s %s\t%s\0' "${mode}" "${new_object_id}" "${path}" >> "${temporary_directory}/index"
  paths+=("${path}")
  outputs+=("${output}")
  if [[ ! -L "${path}" && -f "${path}" ]] && cmp -s -- "${path}" "${source}"; then
    update_working+=(yes)
  else
    update_working+=(no)
  fi
done < "${temporary_directory}/paths"

[[ "${#paths[@]}" -gt 0 ]] || exit 0
git update-index -z --index-info < "${temporary_directory}/index"
for index in "${!paths[@]}"; do
  if [[ "${update_working[index]}" == yes ]]; then
    cat -- "${outputs[index]}" > "${paths[index]}"
  else
    printf 'Preserved unstaged edits in %s\n' "${paths[index]}"
  fi
done
