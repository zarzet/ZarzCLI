#!/usr/bin/env node

const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const REPO = 'zarzet/zarzcli';
const PACKAGE_VERSION = require('../package.json').version;

function getPlatformInfo() {
  const platform = process.platform;
  const arch = process.arch;

  let binaryName = 'zarzcli';
  let platformName;
  let archName;

  // Determine platform
  if (platform === 'win32') {
    platformName = 'windows';
    binaryName += '.exe';
  } else if (platform === 'darwin') {
    platformName = 'macos';
  } else if (platform === 'linux') {
    platformName = 'linux';
  } else {
    return null;
  }

  // Determine architecture
  if (arch === 'x64') {
    archName = 'x86_64';
  } else if (arch === 'arm64') {
    archName = 'aarch64';
  } else {
    return null;
  }

  const assetName = `zarzcli-${platformName}-${archName}${platform === 'win32' ? '.exe' : ''}`;

  return {
    binaryName,
    assetName,
    platform,
    arch
  };
}

function getBinaryPath() {
  const rootDir = path.join(__dirname, '..');
  const platform = process.platform;

  let binaryName = 'zarzcli';
  if (platform === 'win32') {
    binaryName += '.exe';
  }

  return path.join(rootDir, 'bin', binaryName);
}

function downloadBinary(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);

    https.get(url, {
      headers: {
        'User-Agent': 'zarz-installer'
      }
    }, (response) => {
      // Handle redirects
      if (response.statusCode === 302 || response.statusCode === 301) {
        file.close();
        fs.unlinkSync(dest);
        return downloadBinary(response.headers.location, dest)
          .then(resolve)
          .catch(reject);
      }

      if (response.statusCode !== 200) {
        file.close();
        fs.unlinkSync(dest);
        return reject(new Error(`Failed to download: HTTP ${response.statusCode}`));
      }

      response.pipe(file);

      file.on('finish', () => {
        file.close();
        resolve();
      });

      file.on('error', (err) => {
        file.close();
        fs.unlinkSync(dest);
        reject(err);
      });
    }).on('error', (err) => {
      file.close();
      if (fs.existsSync(dest)) {
        fs.unlinkSync(dest);
      }
      reject(err);
    });
  });
}

function makeExecutable(filePath) {
  if (process.platform !== 'win32') {
    try {
      fs.chmodSync(filePath, 0o755);
    } catch (error) {
      console.warn('Warning: Could not make binary executable:', error.message);
    }
  }
}


async function main() {
  console.log('Installing ZarzCLI...');
  console.log('');

  const platformInfo = getPlatformInfo();

  if (!platformInfo) {
    console.error(`Platform ${process.platform}-${process.arch} is not supported.`);
    console.error('');
    console.error('Supported platforms:');
    console.error('  - Linux (x86_64, ARM64)');
    console.error('  - macOS (x86_64, ARM64)');
    console.error('  - Windows (x86_64)');
    console.error('');
    console.error('Please open an issue at: https://github.com/zarzet/zarzcli/issues');
    process.exit(1);
  }

  const binaryPath = getBinaryPath();

  // Check if binary already exists
  if (fs.existsSync(binaryPath)) {
    console.log('Binary already installed.');
    console.log('');
    console.log('ZarzCLI ready to use!');
    console.log('Run "zarz" to start.');
    console.log('');
    return;
  }

  // Ensure bin directory exists
  const binDir = path.dirname(binaryPath);
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  // Try to download pre-built binary
  console.log('Downloading pre-built binary...');

  // Try latest release first, then fall back to version-specific
  const version = PACKAGE_VERSION.replace('-alpha', '').replace('-beta', '');
  const urls = [
    `https://github.com/${REPO}/releases/latest/download/${platformInfo.assetName}`,
    `https://github.com/${REPO}/releases/download/v${version}/${platformInfo.assetName}`
  ];

  let downloaded = false;
  for (const url of urls) {
    try {
      console.log(`Trying: ${url}`);
      await downloadBinary(url, binaryPath);
      makeExecutable(binaryPath);
      downloaded = true;
      console.log('');
      console.log('✓ Download successful!');
      break;
    } catch (error) {
      console.log(`  Failed: ${error.message}`);
      if (fs.existsSync(binaryPath)) {
        fs.unlinkSync(binaryPath);
      }
    }
  }

  if (downloaded) {
    console.log('');
    console.log('ZarzCLI installed successfully!');
    console.log('');
    console.log('Quick Start:');
    console.log('  1. Set API key: export ANTHROPIC_API_KEY=sk-ant-...');
    console.log('  2. Run: zarz');
    console.log('');
    console.log('See QUICKSTART.md for more information.');
    console.log('');
  } else {
    console.error('');
    console.error('Failed to download pre-built binary.');
    console.error('');
    console.error('This could be due to:');
    console.error('  1. Network connection issues');
    console.error('  2. GitHub release not yet available for this version');
    console.error('');
    console.error('Please try:');
    console.error('  1. Check your internet connection');
    console.error('  2. Try again later');
    console.error('  3. Report issue at: https://github.com/zarzet/zarzcli/issues');
    console.error('');
    process.exit(1);
  }
}

main().catch((error) => {
  console.error('Installation failed:', error);
  process.exit(1);
});
