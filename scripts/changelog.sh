#!/usr/bin/env bash
# Viper Music's changelog: user-facing notes, written in plain words. docs/changelog-style.md has the
# rules and docs/release.md how it fits into a release.
#
#   scripts/changelog.sh hook MSGFILE       The commit-msg hook: a commit that changes the app needs
#                                           a `Changelog: <Group>: <text>` paragraph for users, or
#                                           `Changelog: none`.
#   scripts/changelog.sh lint               Checks the entry on stdin against the style rules.
#   scripts/changelog.sh check FILE VERSION Checks a release section for VERSION.
#   scripts/changelog.sh draft TAG FILE     Writes TAG's section into FILE from the Changelog:
#                                           paragraphs since the last tag, unless FILE exists.
#   scripts/changelog.sh pending            Succeeds when a commit since the last tag has a
#                                           Changelog: paragraph other than `none`.
#   scripts/changelog.sh insert FILE VERSION
#                                           Adds VERSION's section in FILE to the top of CHANGELOG.md,
#                                           merging in the sections of tags that never published.
#   scripts/changelog.sh section TAG        Prints TAG's section of CHANGELOG.md.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

STYLE=docs/changelog-style.md
# Changes to these paths are internal and never need an entry. Keep in step with is_app_path.
INTERNAL_PATHS=(':!.github' ':!scripts' ':!docs' ':!*.md')
# The groups, in the order a section lists them. A paragraph names one; one that names none (from
# before groups were required) is Changed.
SECTIONS=(Added Changed Fixed Removed Security)
# What a release with only internal changes says.
NOTHING_NEW="Behind-the-scenes improvements, with nothing new to see."
# Product and platform names whose capitals the identifier checks would flag.
NAMES='GitHub|AppImage|PipeWire|PulseAudio|YouTube|macOS'

die() {
  echo "$*" >&2
  exit 1
}

is_app_path() {
  case "$1" in
    .github/* | scripts/* | docs/* | *.md) return 1 ;;
    *) return 0 ;;
  esac
}

# The text of a message's `Changelog:` paragraph, joined into one line, or nothing.
paragraph() {
  awk '
    /^Changelog:/ { on = 1; sub(/^Changelog:[ \t]*/, "") }
    on && (/^[ \t]*$/ || /^[A-Za-z-]+-[Bb]y:/) { exit }
    on { printf "%s%s", (n++ ? " " : ""), $0 }
  ' <<< "$1" | sed 's/[[:space:]]*$//'
}

is_none() {
  [[ "$1" =~ ^[Nn][Oo][Nn][Ee]\.?$ ]]
}

# The group a paragraph names, or nothing.
group_of() {
  local group
  for group in "${SECTIONS[@]}"; do
    [[ "$1" == "$group: "* ]] && echo "$group" && return
  done
  return 0
}

# Prints what breaks the style rules in one entry, or nothing.
problems() {
  local text="$1" bare
  bare=$(sed -E "s/($NAMES)//g" <<< "$text")
  [[ "$text" == *$'\n'* ]] && echo "it has a line break"
  [[ ${#text} -lt 25 ]] && echo "it is too short to say what changed"
  [[ "$text" == *'`'* ]] && echo "it has backticks"
  grep -qE '[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+\.[a-z]{1,5}\b|\b[A-Za-z0-9_-]+\.(tsx?|jsx?|mjs|rs|toml|json|ya?ml|sh|css|html|md)\b' <<< "$bare" &&
    echo "it names a file"
  grep -qE '\b[a-z0-9]+_[a-z0-9_]+\b' <<< "$bare" && echo "it has a snake_case name"
  grep -qE '\b[a-z]+[A-Z][A-Za-z0-9]*\b|\b[A-Z][a-z0-9]+[A-Z][A-Za-z0-9]*\b' <<< "$bare" &&
    echo "it has a code-style name"
  grep -qE '\b[0-9]+ ?px\b' <<< "$text" && echo "it gives a pixel size"
  grep -qiE '\b(refactor(ed|ing|s)?|components?|backend|frontend|crates?|css|ipc|stdout|stderr|props)\b' <<< "$text" &&
    echo "it uses developer words"
  return 0
}

hook() {
  local file="$1" msg subject text found group
  msg=$(tr -d '\r' < "$file" | grep -v '^#' || true)
  subject=$(head -n 1 <<< "$msg")
  [[ "$subject" =~ ^(Release\ v|Merge\ |fixup!\ |squash!\ |amend!\ ) ]] && return 0
  # An amend's commit also holds what the commit it replaces changed.
  local base=() path app=()
  [[ "${GIT_REFLOG_ACTION:-}" == *amend* ]] && git rev-parse --quiet --verify HEAD^ > /dev/null && base=(HEAD^)
  while IFS= read -r path; do
    [[ -n "$path" ]] && is_app_path "$path" && app+=("$path")
  done < <(git diff --cached --name-only ${base[@]+"${base[@]}"})
  [[ ${#app[@]} -eq 0 ]] && return 0
  found=$(grep -c '^Changelog:' <<< "$msg" || true)
  if [[ "$found" -eq 0 ]]; then
    cat >&2 << EOF
This commit changes the app (${app[0]}$([[ ${#app[@]} -gt 1 ]] && echo ", and $((${#app[@]} - 1)) more")),
so its message needs a paragraph for the changelog, which users read on the GitHub release:

  Changelog: <${SECTIONS[*]// /|}>: <what users see or can do now, where, and what is different>

or, for a change users can't notice, "Changelog: none". The rules are in $STYLE.
EOF
    return 1
  fi
  text=$(paragraph "$msg")
  is_none "$text" && return 0
  group=$(group_of "$text")
  [[ -n "$group" ]] || die "The Changelog: paragraph needs its group first, one of ${SECTIONS[*]}: \"Changelog: Fixed: …\"."
  local issues
  issues=$(problems "${text#"$group: "}")
  if [[ -n "$issues" ]]; then
    {
      echo "The Changelog: paragraph breaks the rules in $STYLE:"
      sed 's/^/  - /' <<< "$issues"
      echo "  $text"
    } >&2
    return 1
  fi
}

lint() {
  local issues
  issues=$(problems "$(cat)")
  [[ -z "$issues" ]] || die "$issues"
}

# Checks a release section: one heading for the version, group names, and each entry.
check() {
  local file="$1" version="${2#v}" line bad=0 issues heads
  [[ -s "$file" ]] || die "$file is empty."
  heads=$(grep -c '^## ' "$file" || true)
  if [[ "$heads" -ne 1 ]] || ! grep -q "^## \[$version\] - [0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}\$" "$file"; then
    echo "It needs exactly one heading, \"## [$version] - YYYY-MM-DD\"." >&2
    bad=1
  fi
  while IFS= read -r line; do
    case "$line" in
      "" | "## ["*) ;;
      "### "*)
        [[ " ${SECTIONS[*]} " == *" ${line#\#\#\# } "* ]] || {
          echo "Unknown group: $line" >&2
          bad=1
        }
        ;;
      "- "*)
        issues=$(problems "${line#- }")
        if [[ -n "$issues" ]]; then
          printf '%s\n' "$line" "$(sed 's/^/    /' <<< "$issues")" >&2
          bad=1
        fi
        ;;
      *)
        echo "Not a heading or an entry: $line" >&2
        bad=1
        ;;
    esac
  done < "$file"
  [[ $bad -eq 0 ]] || die "Fix $file, then run the release again."
}

# The commits since the last tag that change the app, oldest first.
app_commits() {
  local since="$1"
  git log --reverse --format='%H %s' "$since..HEAD" -- . "${INTERNAL_PATHS[@]}" |
    grep -vE '^[0-9a-f]+ Release v' | cut -d' ' -f1 || true
}

# Prints VERSION's section from "group<TAB>entry" lines on stdin: the groups in order, each entry
# once, and the placeholder only when there is nothing else.
render() {
  local version="$1" lines group
  lines=$(awk -F'\t' '!seen[$0]++')
  printf '## [%s] - %s\n' "$version" "$(date +%F)"
  grep -qv $'\t'"$NOTHING_NEW\$" <<< "$lines" && lines=$(grep -v $'\t'"$NOTHING_NEW\$" <<< "$lines")
  [[ -n "$lines" ]] || lines="Changed"$'\t'"$NOTHING_NEW"
  for group in "${SECTIONS[@]}"; do
    grep -q "^$group"$'\t' <<< "$lines" || continue
    printf '\n### %s\n\n' "$group"
    grep "^$group"$'\t' <<< "$lines" | cut -f2- | sed 's/^/- /'
  done
}

# A section as "group<TAB>entry" lines.
entries() {
  awk '/^### / { group = substr($0, 5); next } /^- / { print group "\t" substr($0, 3) }'
}

# The Changelog: paragraphs since the last tag, other than `none`, as "group<TAB>entry" lines.
unreleased() {
  local since hash text group
  since=$(git describe --tags --abbrev=0 --match 'v[0-9]*' HEAD) || die "No earlier release tag."
  while IFS= read -r hash; do
    text=$(paragraph "$(git show -s --format=%B "$hash")")
    [[ -z "$text" ]] || is_none "$text" && continue
    group=$(group_of "$text")
    printf '%s\t%s\n' "${group:-Changed}" "${text#"${group:+$group: }"}"
  done < <(app_commits "$since")
}

draft() {
  local tag="$1" file="$2" version="${1#v}"
  if [[ -s "$file" ]]; then
    echo "Using the notes already written in $file."
    check "$file" "$version"
    return
  fi
  mkdir -p "$(dirname "$file")"
  unreleased | render "$version" > "$file"
  check "$file" "$version"
}

# Succeeds when a commit since the last tag has a Changelog: paragraph for users.
pending() {
  local entries
  entries=$(unreleased)
  [[ -n "$entries" ]]
}

# The version of the newest release that published, from its GitHub release, which the Release
# workflow creates only once every file is up. Nothing when it can't be told.
published() {
  local tag
  tag=$(gh release view --json tagName -q .tagName 2> /dev/null) || return 0
  echo "${tag#v}"
}

insert() {
  local file="$1" version="${2#v}" latest header superseded rest
  check "$file" "$version"
  latest=$(published)
  header=$(awk '/^## \[/ { exit } { print }' CHANGELOG.md)
  # Sections above the newest published release are of tags whose build failed, so users never got
  # them. Their entries go into this one, and they go.
  if [[ -n "$latest" ]] && grep -q "^## \[$latest\]" CHANGELOG.md; then
    superseded=$(awk -v head="## [$latest]" 'index($0, head) == 1 { exit } found || /^## \[/ { found = 1; print }' CHANGELOG.md)
    rest=$(awk -v head="## [$latest]" 'found || index($0, head) == 1 { found = 1; print }' CHANGELOG.md)
  else
    superseded=""
    rest=$(awk 'found || /^## \[/ { found = 1; print }' CHANGELOG.md)
  fi
  if [[ -n "$superseded" ]]; then
    { entries < "$file"; entries <<< "$superseded"; } | render "$version" > "$file.merged"
    mv "$file.merged" "$file"
    check "$file" "$version"
  fi
  printf '%s\n\n%s\n\n%s\n' "$header" "$(cat "$file")" "$rest" > CHANGELOG.md
}

section() {
  local version="${1#v}"
  awk -v head="## [$version]" '
    index($0, head) == 1 { on = 1; print; next }
    on && /^## \[/ { exit }
    on { print }
  ' CHANGELOG.md | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
}

case "${1:-}" in
  hook) hook "${2:?}" ;;
  lint) lint ;;
  check) check "${2:?}" "${3:?}" ;;
  draft) draft "${2:?}" "${3:?}" ;;
  pending) pending ;;
  insert) insert "${2:?}" "${3:?}" ;;
  section) section "${2:?}" ;;
  *) sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//' >&2 && exit 1 ;;
esac
