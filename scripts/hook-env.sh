# Sourced by lefthook before each git hook command (see lefthook.yml).
# Editors started before Rust or uv were installed keep an old PATH, so add their usual homes.
local_app_data="$(cygpath -u "${LOCALAPPDATA:-}" 2>/dev/null || printf '%s' "${LOCALAPPDATA:-}")"
for dir in "$HOME/.cargo/bin" \
  "$local_app_data/Microsoft/WinGet/Links" \
  "$local_app_data"/Microsoft/WinGet/Packages/astral-sh.uv_*; do
  if [ -d "$dir" ]; then
    PATH="$dir:$PATH"
  fi
done
export PATH
