#!/usr/bin/env bash
# Brings Autocue's git history into this repository under autocue/.
#
# Autocue's own repository keeps private material (analysis exports of a personal music
# library, session logs, account names) that must not reach this public repository. This
# script clones Autocue, rewrites the clone with git-filter-repo to drop those files, redact
# personal identifiers and move everything under autocue/, then replays the commits the
# current branch does not have yet on top of it, oldest first, so history stays linear. A
# commit counts as present when one with the same author and author date is already in the
# branch, since imported subjects may have been reworded. Replaying is a cherry-pick, so
# changes made to autocue/ on this side are kept and conflicts stop the script for review.
#
# The working tree must be clean.
#
# Usage: scripts/import-autocue.sh <path-or-url-of-autocue-repo> [branch]
# Needs git-filter-repo (pip install git-filter-repo==2.47.0).
set -euo pipefail

src="${1:?usage: scripts/import-autocue.sh <autocue repo> [branch]}"
branch="${2:-main}"
repo_root="$(git rev-parse --show-toplevel)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if git filter-repo --version >/dev/null 2>&1; then
    filter_repo=(git filter-repo)
elif python -c "import git_filter_repo" >/dev/null 2>&1; then
    filter_repo=(python -m git_filter_repo)
else
    echo "git-filter-repo not found: pip install git-filter-repo==2.47.0" >&2
    exit 1
fi

git clone --quiet --no-local --single-branch --branch "$branch" "$src" "$work/autocue"

# Files that hold personal library data or session records, at their paths in Autocue.
cat >"$work/drop-paths.txt" <<'EOF'
analysis.json
analysis_rb.json
claude_spec.json
gemini.json
gemini_agent_partial.json
gemini_old_prompt.json
new3.json
new3b.json
new3b_paths.json
review_new3.csv
review_new3b.csv
review_rb.csv
mixplan.csv
mixset.csv
run_gemini.cmd
spec_reads_mp3.txt
docs/DOWNLOAD.md
docs/SESSION_TIMELINE.md
docs/TRACKS.md
docs/SPECTROGRAM_READS.md
docs/GEMINI_ATTEMPTS.md
docs/HANDOFF_2026-09.md
EOF

cat >"$work/replace.txt" <<'EOF'
waternlava@gmail.com==>[personal account]
arcanistzed@gmail.com==>[personal account]
C:\Users\water==>%USERPROFILE%
C:/Users/water==>%USERPROFILE%
/mnt/c/Users/water==>$HOME
/Users/water/==>~/
EOF

(
    cd "$work/autocue"
    "${filter_repo[@]}" --force --quiet \
        --invert-paths --paths-from-file "$work/drop-paths.txt" \
        --replace-text "$work/replace.txt" \
        --to-subdirectory-filter autocue
    if git grep -I -q -i -E 'waternlava|arcanistzed@|users[\\/]+water' $(git rev-list --all); then
        echo "personal identifiers remain after the rewrite; not importing" >&2
        exit 1
    fi
)

cd "$repo_root"
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "commit or stash your changes before importing" >&2
    exit 1
fi
git fetch --quiet "$work/autocue" "$branch"
present="$(git log --format='%an <%ae> %ad' --date=raw HEAD -- autocue)"
replayed=0
for commit in $(git rev-list --reverse FETCH_HEAD); do
    who="$(git log -1 --format='%an <%ae> %ad' --date=raw "$commit")"
    if grep -qxF "$who" <<<"$present"; then
        continue
    fi
    git cherry-pick "$commit"
    replayed=$((replayed + 1))
done
echo "replayed $replayed new Autocue commit(s)"
