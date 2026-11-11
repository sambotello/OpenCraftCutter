#!/usr/bin/env bash
# Comprobación completa antes de un commit: formato, clippy y todas las pruebas (unitarias,
# de flujo, de interfaz con egui_kittest y «fotografías» de GPGL y materiales).
# Con INSTALL_HOOK=1 se instala como gancho pre-commit de git.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ "${INSTALL_HOOK:-0}" = 1 ]; then
  printf '#!/usr/bin/env bash\nexec scripts/check.sh\n' > .git/hooks/pre-commit
  chmod +x .git/hooks/pre-commit
  echo "Gancho pre-commit instalado."
  exit 0
fi
echo "==> Formato"
cargo fmt --all -- --check
echo "==> Clippy"
cargo clippy --workspace --all-targets -- -D warnings
echo "==> Pruebas"
cargo test --workspace
echo "Todo correcto."
