#!/bin/sh
# Which path spellings git refuses as `.git`, and what its case-insensitive
# pathspec folds. Run in an empty directory: it creates a repository there.
# Throwaway code for probe P-29.
set -u
for v in $(git rev-parse --local-env-vars 2>/dev/null); do unset "$v"; done
git --version
git init -q . || exit 1
blob=$(echo x | git hash-object -w --stdin)

try() {
    path=$1; shift
    if out=$(git "$@" update-index --add --cacheinfo "100644,$blob,$path" 2>&1); then
        printf '  %-28s accepted\n' "$(printf '%s' "$path" | LC_ALL=C sed 's/[^ -~]/?/g')"
    else
        printf '  %-28s refused\n' "$(printf '%s' "$path" | LC_ALL=C sed 's/[^ -~]/?/g')"
    fi
}

zwnj=$(printf '\342\200\214')   # U+200C
bom=$(printf '\357\273\277')    # U+FEFF
kelvin=$(printf '\342\204\252') # U+212A
echo "no setting given (this platform's defaults)"
for p in ".GIT/x" ".g${zwnj}it/x" "git~1/x" ".git /x"; do try "$p"; done
rm -f .git/index
for hfs in true false; do
    for ntfs in true false; do
        echo "core.protectHFS=$hfs core.protectNTFS=$ntfs"
        for p in ".GIT/x" "a/.Git/x" ".g${zwnj}it/x" ".git${bom}/x" "git~1/x" ".git /x" ".git./x" "wor${kelvin}flows/x" "AGENTS.md${zwnj}" "AGENTS.md." "AGENTS.md " "AGENTS~1.MD"; do
            try "$p" -c core.protectHFS=$hfs -c core.protectNTFS=$ntfs
        done
        rm -f .git/index
    done
done

echo "settings of this clone (empty: not set):"
for k in core.protectHFS core.protectNTFS core.ignorecase core.precomposeunicode; do
    printf '  %s=%s\n' "$k" "$(git config --get $k)"
done

echo "pathspec :(icase), core.precomposeunicode=false; index holds AGENTS.md, <C9>.md (NFC), stra<DF>e, e<U+0301>.txt (NFD)"
g() { git -c core.precomposeunicode=false -c core.quotepath=true "$@"; }
g update-index --add --cacheinfo "100644,$blob,AGENTS.md" \
    --cacheinfo "100644,$blob,$(printf '\303\211.md')" \
    --cacheinfo "100644,$blob,$(printf 'stra\303\237e')" \
    --cacheinfo "100644,$blob,$(printf 'e\314\201.txt')"
for q in "agents.MD" "$(printf '\303\251.md')" "STRASSE" "$(printf '\303\251.txt')" "$(printf 'E\314\201.txt')"; do
    printf '  %-16s -> %s\n' "$(printf '%s' "$q" | LC_ALL=C sed 's/[^ -~]/?/g')" "$(g ls-files -- ":(icase)$q" | tr '\n' ' ')"
done
