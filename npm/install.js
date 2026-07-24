const fs = require("fs");
const path = require("path");
const https = require("https");
const { execSync } = require("child_process");

const VERSION = "0.1.0"; // GitHub Release tag to download from
const BINARY_NAME = "graftail";
const BIN_DIR = path.join(__dirname, "bin");
const BINARY_PATH = path.join(BIN_DIR, process.platform === "win32" ? "graftail.exe" : "graftail");

// Map platform+arch to release asset name
function getAssetName() {
  const platform = process.platform; // linux, darwin, win32
  const arch = process.arch; // x64, arm64

  const map = {
    "linux-x64": "graftail-linux-x64",
    "linux-arm64": "graftail-linux-arm64",
    "darwin-x64": "graftail-darwin-x64",
    "darwin-arm64": "graftail-darwin-arm64",
    "win32-x64": "graftail-win-x64.exe",
  };

  const key = `${platform}-${arch}`;
  const name = map[key];
  if (!name) {
    throw new Error(`Unsupported platform: ${platform}-${arch}`);
  }
  return name;
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https
      .get(url, { headers: { "User-Agent": "graftail-installer" } }, (response) => {
        // Follow redirects
        if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
          return download(response.headers.location, dest).then(resolve, reject);
        }
        if (response.statusCode !== 200) {
          file.close();
          fs.unlinkSync(dest);
          return reject(new Error(`HTTP ${response.statusCode} from ${url}`));
        }
        response.pipe(file);
        file.on("finish", () => {
          file.close();
          resolve();
        });
      })
      .on("error", (err) => {
        fs.unlinkSync(dest);
        reject(err);
      });
  });
}

async function main() {
  // Skip if real binary already installed (> 1MB, not the wrapper)
  if (fs.existsSync(BINARY_PATH) && fs.statSync(BINARY_PATH).size > 1024 * 1024) {
    console.log("[graftail] Binary already present, skipping download.");
    return;
  }

  const assetName = getAssetName();
  const url = `https://github.com/ipconfiger/Logress/releases/download/v${VERSION}/${assetName}`;

  console.log(`[graftail] Downloading ${assetName} from GitHub Releases...`);

  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }

  try {
    await download(url, BINARY_PATH);
    fs.chmodSync(BINARY_PATH, 0o755);
    console.log(`[graftail] Installed successfully: ${BINARY_PATH}`);
  } catch (err) {
    console.error(`[graftail] Download failed: ${err.message}`);
    console.error(`[graftail] You can install manually from: ${url}`);
    process.exit(1);
  }
}

main();
