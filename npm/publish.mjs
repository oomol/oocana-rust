import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { __dirname } from "./utils.mjs";
import { argv } from "node:process";


const registry = argv[2];

console.log("Publishing packages... to", registry || "github");
for (const pkg of fs.readdirSync(path.join(__dirname))) {
    if (pkg.startsWith("oocana-cli")) {

        if (registry == "npm") {
            const { status } = spawnSync("npm", ["pkg", "set", "publishConfig.registry=https://registry.npmjs.org"], {
                cwd: path.join(__dirname, pkg),
                stdio: "inherit",
            });
            if (status !== 0) {
                process.exit(status);
            }
        }

        const { status } = spawnSync("npm", ["publish"], {
            cwd: path.join(__dirname, pkg),
            stdio: "inherit",
        })
        if (status !== 0) {
            process.exit(status);
        }
    }
}