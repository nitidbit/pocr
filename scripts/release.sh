#!/usr/bin/env bash
# Cut a POCR release: tag the current commit and push the tag to GitHub, which
# runs .github/workflows/release.yml (images, signatures, GitHub Release with
# the install bundle).
#
#   scripts/release            # suggests the next patch version
#   scripts/release 0.3.0      # or name it (a leading "v" is fine)
#
# Releases from whatever branch is checked out. Needs a clean working tree.
# With the GitHub CLI (gh) installed, it then follows the workflow run.
set -euo pipefail

remote=origin
workflow=release.yml

die() { echo "release: $*" >&2; exit 1; }
confirm() {
  local answer
  read -r -p "$1 [y/N] " answer
  [[ $answer == [yY]* ]]
}

cd "$(git rev-parse --show-toplevel)"

[[ -z $(git status --porcelain) ]] || die "working tree has uncommitted changes; commit or stash them first."

git fetch --quiet --tags "$remote"
last=$(git tag --list 'v[0-9]*' --sort=-version:refname | head -n 1)

version=${1:-}
if [[ -z $version ]]; then
  suggested=0.1.0
  if [[ -n $last ]]; then
    IFS=. read -r major minor patch <<<"${last#v}"
    suggested="$major.$minor.$((patch + 1))"
  fi
  read -r -p "Version (last is ${last:-none}) [$suggested]: " version
  version=${version:-$suggested}
fi
version=${version#v}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must look like 1.2.3, got '$version'."
tag="v$version"

git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "tag $tag already exists."

branch=$(git rev-parse --abbrev-ref HEAD)
commit=$(git rev-parse --short HEAD)

echo
echo "Release:  $tag"
echo "Branch:   $branch"
echo "Commit:   $commit $(git log -1 --format=%s)"
if [[ -n $last ]]; then
  echo "Changes since $last:"
  git log --oneline "$last..HEAD" | sed 's/^/  /'
fi
echo

if [[ -z $(git branch -r --contains HEAD | grep " $remote/" || true) ]]; then
  echo "This commit is not on $remote yet."
  if confirm "Push branch $branch to $remote first?"; then
    git push "$remote" "$branch"
  fi
fi

confirm "Tag $commit as $tag and push it to $remote? This publishes the release." || die "cancelled."

git tag -a "$tag" -m "POCR $version"
git push "$remote" "$tag"

slug=$(git remote get-url "$remote" | sed -E 's#(\.git)?$##; s#^.*github\.com[:/]##')
echo
echo "Pushed $tag. The release workflow is running:"
echo "  https://github.com/$slug/actions/workflows/$workflow"

if command -v gh >/dev/null; then
  echo "Waiting for the run to start..."
  run=""
  for _ in $(seq 1 12); do
    run=$(gh run list --repo "$slug" --workflow "$workflow" --branch "$tag" --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || true)
    [[ -n $run ]] && break
    sleep 5
  done
  if [[ -n $run ]]; then
    gh run watch "$run" --repo "$slug" --exit-status
    echo
    echo "Released: https://github.com/$slug/releases/tag/$tag"
  else
    echo "Could not find the run; check the link above."
  fi
fi
