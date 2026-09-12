{
  sub(/^[[:space:]]+/, "")
  if ($0 ~ /^(rem|@echo|:|\))/ || $0 == "") next
  for (;;) {
    if (match($0, /^if[[:space:]]+(not[[:space:]]+)?exist[[:space:]]+("[^"]*"|[^[:space:]]+)[[:space:]]+/) ||
        match($0, /^if[[:space:]]+(not[[:space:]]+)?errorlevel[[:space:]]+[0-9]+[[:space:]]+/) ||
        match($0, /^if[[:space:]]+(not[[:space:]]+)?defined[[:space:]]+[^[:space:]]+[[:space:]]+/) ||
        match($0, /^if[[:space:]]+(\/i[[:space:]]+)?(not[[:space:]]+)?"[^"]*"=="[^"]*"[[:space:]]+/) ||
        match($0, /^for[[:space:]]+(\/[dflrDFLR][[:space:]]+("[^"]*"[[:space:]]+)?)?%%[A-Za-z][[:space:]]+in[[:space:]]+\([^)]*\)[[:space:]]+do[[:space:]]+/)) {
      $0 = substr($0, RLENGTH + 1)
      continue
    }
    break
  }
  # The caller verifies this packaged comparator's build/call wiring first.
  if ($1 == "\"%DRV%\\..\\bv-file-compare.exe\"") next
  word = tolower($1)
  sub(/^[^[:alnum:]]+/, "", word)
  if (word ~ /^[a-z]/) print word
}
