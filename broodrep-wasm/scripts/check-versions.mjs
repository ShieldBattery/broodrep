import fs from 'node:fs'
import path from 'node:path'

const wasmDir = path.resolve(import.meta.dirname, '..')
const repoRoot = path.resolve(wasmDir, '..')

function cargoPackageVersion(manifestPath) {
  const lines = fs.readFileSync(manifestPath, 'utf8').split(/\r?\n/)
  let inPackageTable = false

  for (const line of lines) {
    const table = line.match(/^\s*\[([^\]]+)]\s*$/)
    if (table) {
      inPackageTable = table[1] === 'package'
      continue
    }

    if (inPackageTable) {
      const version = line.match(/^\s*version\s*=\s*"([^"]+)"/)
      if (version) return version[1]
    }
  }

  throw new Error(`No package version found in ${manifestPath}`)
}

const packageJsonPath = path.join(wasmDir, 'package.json')
const packageJsonVersion = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8')).version
const crateManifests = [
  path.join(repoRoot, 'broodrep', 'Cargo.toml'),
  path.join(repoRoot, 'broodrep-cli', 'Cargo.toml'),
  path.join(repoRoot, 'broodrep-wasm', 'Cargo.toml'),
]

const mismatches = crateManifests
  .map(manifestPath => ({
    manifestPath,
    version: cargoPackageVersion(manifestPath),
  }))
  .filter(({ version }) => version !== packageJsonVersion)

if (mismatches.length) {
  const details = mismatches
    .map(
      ({ manifestPath, version }) =>
        `  ${path.relative(repoRoot, manifestPath)}: ${version} (expected ${packageJsonVersion})`,
    )
    .join('\n')
  throw new Error(`Package versions are not aligned:\n${details}`)
}

console.log(`Package versions are aligned at ${packageJsonVersion}`)
