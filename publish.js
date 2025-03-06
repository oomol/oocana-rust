const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

for (const pkg of fs.readdirSync(path.join(__dirname, "npm"))) {
    if (pkg.startsWith("vocana-cli")) {
        spawnSync("npm", ["publish"], {
            cwd: path.join(__dirname, "npm", pkg),
            stdio: "inherit",
        });
    }
}