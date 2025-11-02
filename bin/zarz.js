#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

// Determine the binary path based on platform
function getBinaryPath() {
  const rootDir = path.join(__dirname, '..');
  const platform = process.platform;

  let binaryName = 'zarzcli';
  if (platform === 'win32') {
    binaryName += '.exe';
  }

  const binaryPath = path.join(rootDir, 'target', 'release', binaryName);

  return binaryPath;
}

// Check if binary exists, if not, build it
function ensureBinaryExists(binaryPath) {
  if (!fs.existsSync(binaryPath)) {
    console.error('Binary not found. Building...');
    console.error('This may take a few minutes on first run.');

    const { execSync } = require('child_process');
    const rootDir = path.join(__dirname, '..');

    try {
      execSync('cargo build --release', {
        cwd: rootDir,
        stdio: 'inherit'
      });
    } catch (error) {
      console.error('Failed to build binary. Make sure Rust and Cargo are installed.');
      console.error('Install Rust from: https://rustup.rs/');
      process.exit(1);
    }
  }
}

// Main execution
function main() {
  const binaryPath = getBinaryPath();

  // Ensure binary exists
  ensureBinaryExists(binaryPath);

  // Check if binary exists after build attempt
  if (!fs.existsSync(binaryPath)) {
    console.error(`Binary not found at: ${binaryPath}`);
    console.error('Please run: cargo build --release');
    process.exit(1);
  }

  // Forward all arguments to the binary
  const args = process.argv.slice(2);

  // Spawn the binary
  const child = spawn(binaryPath, args, {
    stdio: 'inherit',
    env: process.env
  });

  // Handle exit
  child.on('exit', (code) => {
    process.exit(code || 0);
  });

  // Handle errors
  child.on('error', (error) => {
    console.error('Failed to start zarz:', error.message);
    process.exit(1);
  });
}

// Run main
main();
