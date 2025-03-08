import fs from "node:fs/promises";
import path from "node:path";
import { packageTemplate, readme, targetConfig } from "./target.mjs";
import { getVersion, __dirname } from "./utils.mjs";

const targets = Object.keys(targetConfig);
const version = await getVersion();

const owner = process.env.GITHUB_REPOSITORY_OWNER || "oomol";
const projectDir = path.dirname(__dirname);

// @oomol/oocana-cli-* package.json with binaries
for (const target of targets) {
    const targetDir = path.join(__dirname, `oocana-cli-${target}`);
    const binDir = path.join(targetDir, "bin");
    await fs.mkdir(binDir, { recursive: true });

    const bin = path.join(binDir, "oocana");
    const oocana = path.join(projectDir, "target", target, "release", "oocana");

    await fs.copyFile(oocana, bin);

    const newFile = {
        name: `@${owner}/oocana-cli-${target}`,
        description: `The ${target} cli binary for @${owner}/oocana`,
        version,
        bin: {
            "oocana": `bin/oocana`,
        },
        os: targetConfig[target].os,
        cpu: targetConfig[target].cpu,
        ...packageTemplate,
    };

    await fs.writeFile(path.join(targetDir, "package.json"), JSON.stringify(newFile, null, 2));
    await fs.writeFile(path.join(targetDir, "README.md"), readme(target).replace(/@oomol/g, `@${owner}`));
    await fs.copyFile(path.join(__dirname, "index.js"), path.join(targetDir, "index.js"));
}

