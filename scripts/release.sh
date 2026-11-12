#!/usr/bin/env bash
# Publica una versión nueva de OpenCraftCutter respetando el ruleset de main (todo por PR):
#
#   scripts/release.sh patch|minor|major
#
# 1. Desde main al día, crea la rama version-X.Y.Z y sube la versión en Cargo.toml.
# 2. Pasa scripts/check.sh, hace el commit «Versión X.Y.Z», lo sube y abre el PR.
# 3. Fusiona el PR con el bypass de admin (squash) y vuelve a main.
# 4. Compila el DMG desde main (scripts/build-dmg.sh) y crea la release vX.Y.Z en GitHub con el
#    DMG adjunto; la etiqueta la crea GitHub sobre el commit de main.
#
# Si algo falla después de fusionar (por ejemplo, al compilar), la versión ya está en main:
# arregla el problema y ejecuta `scripts/release.sh publicar`, que solo hace el paso 4.
#
# Notas de la release: las genera GitHub con los PRs fusionados desde la versión anterior, tras
# las instrucciones de instalación. Con NOTES=archivo.md se añade tu texto al principio.
# SIGN_ID, NOTARY_PROFILE y UNIVERSAL se pasan tal cual a build-dmg.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=OpenCraftCutter
MODE="${1:-}"
case "$MODE" in patch | minor | major | publicar) ;; *)
  echo "Uso: scripts/release.sh patch|minor|major   (o «publicar» para reintentar solo el DMG y la release)" >&2
  exit 1
  ;;
esac

die() {
  echo "Error: $*" >&2
  exit 1
}
version() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1; }

gh auth status >/dev/null 2>&1 || die "gh no tiene sesión iniciada (gh auth login)."
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)

# --- Comprobaciones: main limpio y al día con GitHub ---
[ "$(git branch --show-current)" = main ] || die "ejecútalo desde main (git switch main)."
[ -z "$(git status --porcelain)" ] || die "hay cambios sin commit."
git fetch -q --prune --tags origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
  die "main no coincide con origin/main (git pull)."

tag_exists() {
  git rev-parse -q --verify "refs/tags/$1" >/dev/null ||
    [ -n "$(git ls-remote --tags origin "refs/tags/$1")" ]
}
# Última versión publicada (para las notas): la etiqueta vX.Y.Z más reciente.
PREV_TAG=$(git tag --list 'v*' --sort=-v:refname | head -1)

if [ "$MODE" != publicar ]; then
  OLD=$(version)
  IFS=. read -r MA MI PA <<<"$OLD"
  case "$MODE" in
    patch) PA=$((PA + 1)) ;;
    minor) MI=$((MI + 1)) PA=0 ;;
    major) MA=$((MA + 1)) MI=0 PA=0 ;;
  esac
  NEW="$MA.$MI.$PA"
  BRANCH="version-$NEW"
  tag_exists "v$NEW" && die "la etiqueta v$NEW ya existe."
  [ -z "$(git ls-remote --heads origin "$BRANCH")" ] || die "la rama $BRANCH ya existe en GitHub."

  echo "==> Versión $OLD → $NEW (rama $BRANCH)"
  git switch -q -c "$BRANCH"
  trap 'echo "Falló antes de fusionar: main no ha cambiado. Para descartar: git switch main && git branch -D $BRANCH (y borra la rama/PR en GitHub si llegó a subirse)." >&2' ERR
  # Solo la primera línea «version = …» (la del workspace).
  sed -i '' "1,/^version = /s/^version = \".*\"/version = \"$NEW\"/" Cargo.toml
  cargo update --workspace --quiet

  echo "==> Comprobaciones (scripts/check.sh)"
  scripts/check.sh

  git add Cargo.toml Cargo.lock
  git commit -q -m "Versión $NEW"
  git push -q -u origin "$BRANCH"

  echo "==> PR y fusión"
  PR_URL=$(gh pr create --base main --head "$BRANCH" --title "Versión $NEW" \
    --body "Sube la versión de $OLD a $NEW. PR creado por \`scripts/release.sh\`.")
  echo "    $PR_URL"
  gh pr merge "$PR_URL" --squash --admin --delete-branch
  trap - ERR

  git switch -q main 2>/dev/null || true
  git pull -q --ff-only origin main
  git branch -D "$BRANCH" >/dev/null 2>&1 || true
  git fetch -q --prune origin
fi

# --- DMG y release desde main ---
VERSION=$(version)
TAG="v$VERSION"
tag_exists "$TAG" && die "$TAG ya está publicada."
trap 'echo "Falló al compilar o publicar. La versión $VERSION ya está en main: arréglalo y ejecuta scripts/release.sh publicar" >&2' ERR

echo "==> DMG de $NAME $VERSION"
scripts/build-dmg.sh
DMG="dist/$NAME-$VERSION.dmg"
[ -f "$DMG" ] || die "no se generó $DMG."
SHA=$(shasum -a 256 "$DMG" | cut -d' ' -f1)

NOTES_FILE=$(mktemp)
trap 'rm -f "$NOTES_FILE"' EXIT
{
  if [ -n "${NOTES:-}" ]; then
    cat "$NOTES"
    echo
  fi
  echo "## Instalación (Mac con Apple Silicon)"
  echo "1. Descarga \`$NAME-$VERSION.dmg\` y arrastra la app a **Aplicaciones**."
  if [ -z "${SIGN_ID:-}" ] || [ -z "${NOTARY_PROFILE:-}" ]; then
    echo "2. La app no está notarizada: la primera vez, haz **clic derecho › Abrir** (o Ajustes del Sistema › Privacidad y seguridad › **Abrir igualmente**)."
  else
    echo "2. Ábrela como cualquier otra app."
  fi
  echo "3. Al conectar la máquina, macOS pedirá permiso de **Bluetooth**. Cierra Silhouette Studio antes: la máquina solo acepta una conexión a la vez."
  echo
  echo "SHA-256 del DMG: \`$SHA\`"
  echo
  echo "¿Algo falla? Abre un [issue](https://github.com/$REPO/issues/new/choose)."
  echo
  echo "Software libre bajo GPL-3.0, sin garantía. Proyecto independiente, no afiliado a Silhouette America, Inc."
  echo
} >"$NOTES_FILE"

echo "==> Release $TAG"
ARGS=(--target "$(git rev-parse HEAD)" --title "$NAME $VERSION" --notes-file "$NOTES_FILE" --generate-notes)
[ -n "$PREV_TAG" ] && [ "$PREV_TAG" != "$TAG" ] && ARGS+=(--notes-start-tag "$PREV_TAG")
gh release create "$TAG" "$DMG" "${ARGS[@]}"
git fetch -q --tags origin
trap - ERR
echo "Listo: https://github.com/$REPO/releases/tag/$TAG"
