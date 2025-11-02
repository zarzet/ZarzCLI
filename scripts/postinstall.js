#!/usr/bin/env node

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function checkRustInstalled() {
  try {
    execSync('cargo --version', { stdio: 'ignore' });
    return true;
  } catch (error) {
    return false;
  }
}

function getBinaryPath() {
  const rootDir = path.join(__dirname, '..');
  const platform = process.platform;

  let binaryName = 'zarzcli';
  if (platform === 'win32') {
    binaryName += '.exe';
  }

  return path.join(rootDir, 'target', 'release', binaryName);
}

function main() {
  console.log('Installing ZarzCLI...');

  // Check if Rust is installed
  if (!checkRustInstalled()) {
    console.log('');
    console.log('WARNING: Rust/Cargo not found!');
    console.log('ZarzCLI requires Rust to build the native binary.');
    console.log('');
    console.log('Please install Rust from: https://rustup.rs/');
    console.log('');
    console.log('After installing Rust, run: npm install');
    console.log('');
    process.exit(0); // Don't fail install, just warn
  }

  const binaryPath = getBinaryPath();

  // Check if binary already exists
  if (fs.existsSync(binaryPath)) {
    console.log('Binary already built. Skipping build.');
    console.log('');
    console.log('ZarzCLI installed successfully!');
    console.log('Run "zarz" to start.');
    return;
  }

  // Build the binary
  console.log('Building native binary...');
  console.log('This may take a few minutes on first install.');
  console.log('');

  try {
    const rootDir = path.join(__dirname, '..');

    execSync('cargo build --release', {
      cwd: rootDir,
      stdio: 'inherit'
    });

    console.log('');
    console.log('ZarzCLI installed successfully!');
    console.log('');
    console.log('Quick Start:');
    console.log('  1. Set API key: export ANTHROPIC_API_KEY=sk-ant-...');
    console.log('  2. Run: zarz');
    console.log('');
    console.log('See QUICKSTART.md for more information.');
    console.log('');

  } catch (error) {
    console.error('');
    console.error('Build failed!');
    console.error('');
    console.error('Please make sure:');
    console.error('  1. Rust and Cargo are installed: https://rustup.rs/');
    console.error('  2. You have an internet connection (to download dependencies)');
    console.error('');
    console.error('Error:', error.message);
    process.exit(1);
  }
}

main();
