import fs from "node:fs/promises";
import path from "node:path";
import { packageTemplate, readme, targetConfig } from "./target.mjs";
import { getVersion, __dirname } from "./utils.mjs";
import { spawn } from "node:child_process";

const targets = Object.keys(targetConfig);
const version = await getVersion();

const owner = process.env.GITHUB_REPOSITORY_OWNER || "oomol";
const projectDir = path.dirname(__dirname);

// build the @oomol/oocana-cli package.json with binaries
if (!process.env.CI) {
    console.log("build outside of CI. be aware the owner is @oomol not your github username!");
    for (const target of targets) {
        const cargo = spawn("cargo", ["build", "--release", "--target",
            target], {
            cwd: projectDir,
            env: {
                ...process.env,
                // you need install gcc for linux targets
                CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER: "x86_64-linux-gnu-gcc",
                CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: "aarch64-linux-gnu-gcc",
            }
        });
        cargo.stdout.pipe(process.stdout);
        cargo.stderr.pipe(process.stderr);
        await new Promise((resolve, reject) => {
            cargo.on("close", (code) => {
                if (code === 0) {
                    resolve();
                } else {
                    reject(new Error(`Cargo build failed for target ${target} with code ${code}`));
                }
            });
        });
        console.log(`Cargo build succeeded for target ${target}`);
    }
}

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
    console.log(`Built @${owner}/oocana-cli-${target} package.json`);

}

