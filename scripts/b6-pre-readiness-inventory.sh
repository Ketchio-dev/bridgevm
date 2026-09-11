# An early observation cannot replace firstboot readiness or later closure proof.
b6_early_status="$OUT/reference-inventory-before-readiness-status.txt"
[[ ! -e "$b6_early_status" && ! -L "$b6_early_status" ]] || return 1
source "$REPO/scripts/b6-collect-reference-inventory.sh"
mv "$OUT/reference-inventory-status.txt" "$b6_early_status"
