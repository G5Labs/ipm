#!/usr/bin/env bash
# Static guard for operations g5.quality.delivery.pinned-third-party-tools
# (G5Labs/G5-Lean#527): no image recipe or privileged bootstrap/install script
# may pipe a network download into a shell, an interpreter or an archive
# extractor. Such a pipeline executes or unpacks whatever the endpoint returns
# with no checksum between the bytes and the sink; the repository pattern is
# download to a file, verify a repository-pinned sha256, then extract or run.
#
# Usage: verify-pinned-downloads.sh [PATH ...]
#   Scans every Dockerfile* and shell script (*.sh, *.bash, or a sh/bash/dash
#   shebang) under each PATH; a PATH may also be a single file (the Justfile
#   is scanned as shell). Defaults to the image recipes, their helper scripts
#   and the self-hosted/Packer/guest install scripts. Comment lines are
#   dropped and backslash continuations joined before matching, so a pipeline
#   split across lines cannot hide and a comment that mentions `curl | sh`
#   does not trip the guard. Exits 1 naming every offending file and line,
#   2 on a missing path or an empty scan, 0 with a summary otherwise.
#
#   Flagged: `curl … | sh`, `wget -qO- … | sudo -E bash -`, `curl … | tar xz`,
#   `curl … | gunzip | tar x`, `curl … | python3 -c '…'` (an interpreter is a
#   sink whatever its arguments: it may execute what it reads), sinks behind
#   launchers such as `sudo -u root bash`, `env -i sh`, `nice -n 10 sh`,
#   `timeout 60 bash` or `busybox sh`, tab-indented or continued pipelines,
#   `sh -c 'curl … | sh'` or `ssh host "curl … | bash"` (a quoted string after
#   a shell-like word is scanned as a command of its own), `sh -c 'curl …' |
#   bash` and `env -S 'FOO="a b" bash -s'` (quoted text is inlined into its
#   enclosing pipeline with its own pipes neutralised and quoted words kept
#   together), `echo "$(curl … | sh)"` or backticks (every command
#   substitution is scanned as a command), and a download fed to a shell or
#   extractor through a substitution: `eval "$(curl …)"`, `bash <(curl …)`,
#   `source <(wget -qO- …)`, `tar xzf <(curl …)`.
#   Not flagged: `curl -o f … && sha256sum -c … && tar xf f`, `curl … | jq`,
#   `curl … || bash fallback.sh`, `curl … | grep -E 'sh|bash'`,
#   `TAG="$(curl … | jq -r .tag)"`, `bash -c "curl -o f … && sha256sum -c …"`.
set -euo pipefail

if [[ $# -eq 0 ]]; then
  set -- infra/docker infra/self-hosted infra/packer infra/guest scripts Justfile
fi

files=()
for root in "$@"; do
  if [[ -f "$root" ]]; then
    files+=("$root")
  elif [[ -d "$root" ]]; then
    while IFS= read -r f; do
      case "$(basename "$f")" in
        Dockerfile*|*.sh|*.bash) files+=("$f") ;;
        *)
          if head -c 200 "$f" 2>/dev/null | head -n 1 | grep -qE '^#![^[:space:]]*/(env[[:space:]]+)?(ba|da|k|z)?sh([[:space:]]|$)'; then
            files+=("$f")
          fi
          ;;
      esac
    done < <(find "$root" \( -name node_modules -o -name target -o -name .git \) -prune -o -type f -print | sort)
  else
    echo "FAIL: pinned-download scan path does not exist: $root" >&2
    exit 2
  fi
done
if [[ ${#files[@]} -eq 0 ]]; then
  echo "FAIL: pinned-download scan found no Dockerfiles or shell scripts under: $*" >&2
  exit 2
fi

fail=0
for f in "${files[@]}"; do
  # Emit "<first line number>\t<logical line>" with comment-only lines dropped
  # (also inside continuations, as Docker and the shell both do) and
  # backslash continuations joined.
  hits="$(awk '
    function flush() {
      if (buf != "") { print start "\t" buf }
      buf=""; start=0; cont=0
    }
    {
      line=$0
      if (line ~ /^[[:space:]]*#/) { next }
      if (!cont) { start=NR; buf="" }
      ends=(line ~ /\\[[:space:]]*$/)
      if (ends) { sub(/\\[[:space:]]*$/, "", line) }
      buf=buf (cont ? " " : "") line
      cont=ends
      if (!cont) { flush() }
    }
    END { if (cont) flush() }
  ' "$f" | awk '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    # Strip the launchers that only relay stdin to the real command (sudo,
    # doas, env, nice, ionice, nohup, time, timeout, exec, command, busybox,
    # setsid, stdbuf, chroot, taskset, flock), consuming the options of each,
    # the options that take a separate value (`sudo -u root`), its operands
    # (`timeout 60`, `chroot /root`) and NAME=value prefixes, so the command
    # that actually reads the download is the one classified.
    # Split a segment into words, keeping a quoted span with its spaces as
    # one word, so env -S FOO="hello world" bash sees one assignment.
    function split_words(seg, tok,   n, i, c, q, cur, inword) {
      n=0; cur=""; q=""; inword=0
      for (i=1; i<=length(seg); i++) {
        c=substr(seg, i, 1)
        if (q != "") { cur=cur c; if (c == q) q=""; continue }
        if (c == sq || c == dq) { q=c; cur=cur c; inword=1; continue }
        if (c ~ /[[:space:]]/) { if (inword) { tok[++n]=cur; cur=""; inword=0 }; continue }
        cur=cur c; inword=1
      }
      if (inword) tok[++n]=cur
      return n
    }
    function strip_wrappers(seg,   n, tok, i, w, opts, left, rest) {
      n=split_words(seg, tok)
      i=1
      while (i <= n) {
        if (tok[i] ~ /^[A-Za-z_][A-Za-z0-9_]*=/) { i++; continue }
        w=tok[i]; sub(/^.*\//, "", w)
        if (!(w in valued)) break
        opts=valued[w]; i++
        while (i <= n && tok[i] ~ /^-/) {
          if (tok[i] == "--") { i++; break }
          if (w == "nice" && tok[i] ~ /^-[0-9]+$/) { i++; continue }
          # GNU env -S / --split-string: the value IS the command line (its
          # quoted text was inlined into these tokens), so classify it.
          if (w == "env" && tok[i] ~ /^-S./) { tok[i]=substr(tok[i], 3); break }
          if (w == "env" && tok[i] ~ /^--split-string=./) { sub(/^--split-string=/, "", tok[i]); break }
          if (w == "env" && (tok[i] == "-S" || tok[i] == "--split-string")) { i++; continue }
          if (tok[i] !~ /=/ && index(opts, " " tok[i] " ")) i+=2; else i++
        }
        left=operands[w]+0
        while (left > 0 && i <= n) { i++; left-- }
      }
      rest=""
      for (; i <= n; i++) rest=rest (rest == "" ? "" : " ") tok[i]
      return rest
    }
    function first_word(seg,   w) {
      w=seg; sub(/[[:space:]].*$/, "", w)
      if (substr(w, 1, 1) == sq || substr(w, 1, 1) == dq) w=substr(w, 2)
      if (substr(w, length(w), 1) == sq || substr(w, length(w), 1) == dq) w=substr(w, 1, length(w)-1)
      sub(/^.*\//, "", w)
      return w
    }
    # The sink is the first command of a pipe segment: a shell, an interpreter
    # (whatever its arguments: it may execute what it reads) or an archive
    # extractor, after wrappers that only relay stdin.
    function is_sink(seg,   w) {
      sub(/[;&].*$/, "", seg)
      seg=strip_wrappers(seg)
      if (seg == "") return 0
      w=first_word(seg)
      return (w ~ /^(sh|bash|dash|ash|ksh|zsh|fish|eval|source|\.|tar|bsdtar|unzip|python[0-9.]*|perl|ruby|node|nodejs|php)$/)
    }
    # The text with every quoted string inlined without its quotes and with
    # its own pipes neutralised: a quoted pattern such as sh|bash cannot split
    # the pipeline, while a quoted download (sh -c \047curl …\047 | bash) or a
    # quoted sink (env -S \047bash -s\047) still belongs to its segment.
    function flatten_quotes(text,   outer, rest, at, q, endq, body) {
      outer=""; rest=text
      while ((at=match(rest, QUOTE_RE)) > 0) {
        q=substr(rest, at, 1)
        outer=outer substr(rest, 1, at-1)
        endq=index(substr(rest, at+1), q)
        if (endq == 0) { body=substr(rest, at+1); rest="" } else { body=substr(rest, at+1, endq-1); rest=substr(rest, at+endq+1) }
        gsub(/\|/, "\002", body)
        outer=outer " " body " "
      }
      return outer rest
    }
    # Does this command text pipe a download into a sink?
    # Does this (flattened) command text pipe a download into a sink, or feed
    # a sink a download through a command or process substitution?
    function piped_download(text,   n, segs, i, seen) {
      gsub(/\|\|/, " \001 ", text)                    # an or-list is not a pipe
      n=split(text, segs, /\|/)
      seen=0
      for (i=1; i<=n; i++) {
        if (seen && is_sink(segs[i])) return 1
        if (segs[i] ~ SUBST_DL_RE && is_sink(segs[i])) return 1
        if (segs[i] ~ DL_RE) seen=1
      }
      return 0
    }
    # Every $(…) and backtick substitution executes its body whatever quotes
    # or command surround it, so each body is scanned as a command.
    function substitutions_piped(text,   n, i, c, depth, start, body, endb) {
      n=length(text)
      for (i=1; i<=n; i++) {
        if (substr(text, i, 2) == "$(") {
          depth=1; start=i+2
          for (i=start; i<=n && depth > 0; i++) { c=substr(text, i, 1); if (c == "(") depth++; else if (c == ")") depth-- }
          body=substr(text, start, i-start-1)
          if (piped_download(flatten_quotes(body))) return 1
          i--
        } else if (substr(text, i, 1) == bt) {
          endb=index(substr(text, i+1), bt)
          if (endb == 0) return 0
          body=substr(text, i+1, endb-1)
          if (piped_download(flatten_quotes(body))) return 1
          i+=endb
        }
      }
      return 0
    }
    # A string quoted with QUOTE that follows a shell-like word in its pipe
    # segment (`sh -c`, `bash -c`, `ssh host`, `su -c`, `eval`) is a command
    # body and is scanned as a command of its own; other quoted text is data
    # (a message that mentions curl | sh is not a pipeline).
    function quoted_body_piped(text, quote,   rest, body, re, at, len, pos, before) {
      re=quote "[^" quote "]*" quote
      rest=text; pos=0
      while (match(rest, re)) {
        # Copy RSTART/RLENGTH first: the nested match() calls below overwrite
        # these globals, which would stall this loop.
        at=RSTART; len=RLENGTH
        body=substr(rest, at+1, len-2)
        before=substr(text, 1, pos+at-1); sub(/.*\|/, "", before)
        if (before ~ SHELL_RE && piped_download(body)) return 1
        pos+=at+len-1
        rest=substr(rest, at+len)
      }
      return 0
    }
    BEGIN {
      sq=sprintf("%c", 39); dq=sprintf("%c", 34)
      DL_RE="(^|[^A-Za-z0-9_./-])(curl|wget)([[:space:]]|$)"
      bt=sprintf("%c", 96)
      QUOTE_RE="[" sq dq "]"
      SUBST_DL_RE="(\\$\\(|<\\(|" bt ")([^)" bt "]*[^A-Za-z0-9_./-])?(curl|wget)([[:space:]]|$)"
      SHELL_RE="(^|[^A-Za-z0-9_./-])(sh|bash|dash|ash|ksh|zsh|fish|ssh|su|eval)([[:space:]]|$)"
      valued["sudo"]=" -u -g -C -D -h -p -r -t -U -T --user --group --close-from --chdir --host --prompt --role --type --other-user --command-timeout "
      valued["doas"]=" -u -C "
      valued["env"]=" -u -C --unset --chdir "
      valued["nice"]=" -n --adjustment "
      valued["ionice"]=" -c -n --class --classdata "
      valued["nohup"]=" "
      valued["time"]=" -f -o --format --output "
      valued["timeout"]=" -k -s --kill-after --signal "
      valued["exec"]=" -a "
      valued["command"]=" "
      valued["busybox"]=" "
      valued["setsid"]=" "
      valued["stdbuf"]=" -i -o -e --input --output --error "
      valued["chroot"]=" --userspec --groups "
      valued["taskset"]=" "
      valued["flock"]=" -w -E --wait --timeout --conflict-exit-code "
      operands["timeout"]=1; operands["chroot"]=1; operands["taskset"]=1; operands["flock"]=1
    }
    {
      # Split at the FIRST tab only: the logical line itself may contain tabs.
      at=index($0, "\t")
      lineno=substr($0, 1, at-1)
      text=substr($0, at+1)
      if (piped_download(flatten_quotes(text)) || substitutions_piped(text) || quoted_body_piped(text, sq) || quoted_body_piped(text, dq)) {
        printf "%s:%s: %s\n", FILENAME_HINT, lineno, text
      }
    }
  ' FILENAME_HINT="$f")"
  if [[ -n "$hits" ]]; then
    while IFS= read -r hit; do
      echo "FAIL: ${hit:0:220}" >&2
    done <<< "$hits"
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "FAIL: a download is piped into a shell, interpreter or archive extractor; download to a file and verify a repository-pinned sha256 first (g5.quality.delivery.pinned-third-party-tools)" >&2
  exit 1
fi
echo "OK: pinned-download scan — ${#files[@]} recipe/script file(s), no download piped into a shell or extractor"
