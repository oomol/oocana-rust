import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { packageTemplate, readme, targetConfig } from "./target.mjs";
import { downloadFile, getVersion } from "./utils.mjs";

const targets = Object.keys(targetConfig);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const version = await getVersion();

const owner = process.env.GITHUB_REPOSITORY_OWNER || "oomol";

// @oocana/oocana-cli package.json without binaries
const oocana_cli_package = path.join(__dirname, "oocana-cli");
const oocana_cli_bin = path.join(oocana_cli_package, "bin");
{
    await fs.mkdir(oocana_cli_bin, { recursive: true });

    const newFile = {
        name: `@${owner}/oocana-cli`,
        description: `All-platform cli binaries for @${owner}/oocana`,
        version,
        ...packageTemplate
    };

    await fs.writeFile(path.join(oocana_cli_package, "package.json"), JSON.stringify(newFile, null, 2));
    await fs.writeFile(path.join(oocana_cli_package, "README.md"), `# @${owner}/oocana-cli\n\n${newFile.description}`);
    await fs.copyFile(path.join(__dirname, "index.js"), path.join(oocana_cli_package, "index.js"));
}

// @oomol/oocana-cli-* package.json with binaries
for (const target of targets) {
    const targetDir = path.join(__dirname, `oocana-cli-${target}`);
    const binDir = path.join(targetDir, "bin");
    await fs.mkdir(binDir, { recursive: true });

    const oocana = (path.join(binDir, "oocana"))

    // TODO: consider build locally if the 404.
    await downloadFile(`https://github.com/${owner}/oocana-rust/releases/download/v${version}/oocana-cli-${target}`, oocana);

    // copy the oocana binary to npm/oocana-cli/bin
    await fs.copyFile(oocana, path.join(oocana_cli_bin, target));

    const newFile = {
        name: `@${owner}/oocana-cli-${target}`,
        description: `The ${target} cli binary for @${owner}/oocana`,
        version,
        bin: {
            "oocana": `bin/${target}`
        },
        os: targetConfig[target].os,
        cpu: targetConfig[target].cpu,
        ...packageTemplate,
    };

    await fs.writeFile(path.join(targetDir, "package.json"), JSON.stringify(newFile, null, 2));
    await fs.writeFile(path.join(targetDir, "README.md"), readme(target).replace(/@oomol/g, `@${owner}`));
    await fs.copyFile(path.join(__dirname, "index.js"), path.join(targetDir, "index.js"));
}

