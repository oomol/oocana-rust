const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

for (const pkg of fs.readdirSync(path.join(__dirname, "npm"))) {
    if (pkg.startsWith("oocana-cli")) {
        const { status } = spawnSync("npm", ["publish"], {
            cwd: path.join(__dirname, "npm", pkg),
            stdio: "inherit",
        })
        if (status !== 0) {
            process.exit(status);
        }
    }
}