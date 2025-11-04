#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

// Determine the binary path based on platform
function getBinaryPath() {
  const platform = process.platform;

  let binaryName = 'zarzcli';
  if (platform === 'win32') {
    binaryName += '.exe';
  }

  // Binary is now in bin/ directory after installation
  const binaryPath = path.join(__dirname, binaryName);

  return binaryPath;
}

// Main execution
function main() {
  const binaryPath = getBinaryPath();

  // Check if binary exists
  if (!fs.existsSync(binaryPath)) {
    console.error('ZarzCLI binary not found!');
    console.error('');
    console.error('This usually means the installation did not complete successfully.');
    console.error('Please try reinstalling:');
    console.error('  npm install -g zarz');
    console.error('');
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
