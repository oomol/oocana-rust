import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { __dirname } from "./utils.mjs";

for (const pkg of fs.readdirSync(path.join(__dirname))) {
    if (pkg.startsWith("oocana-cli")) {
        const { status } = spawnSync("npm", ["publish"], {
            cwd: path.join(__dirname, pkg),
            stdio: "inherit",
        })
        if (status !== 0) {
            process.exit(status);
        }
    }
}