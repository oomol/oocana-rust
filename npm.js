const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const targetConfig = {
    "x86_64-apple-darwin": {
        os: "darwin",
        cpu: "x64",
    },
    "aarch64-apple-darwin": {
        os: "darwin",
        cpu: "arm64",
    },
    "x86_64-unknown-linux-gnu": {
        os: "linux",
        cpu: "x64",
    },
    "aarch64-unknown-linux-gnu": {
        os: "linux",
        cpu: "arm64",
    },
    // "x86_64-unknown-linux-musl": {
    //     os: "linux",
    //     cpu: "x64",
    // }
};
const targets = Object.keys(targetConfig);

const version = getVersion();

const NPM_PATH = path.join(__dirname, "npm");

if (fs.existsSync(NPM_PATH)) {
    // clean npm directory
    for (const file of fs.readdirSync(NPM_PATH)) {
        fs.rmSync(path.join(__dirname, "npm", file), { recursive: true });
    }
} else {
    fs.mkdirSync(NPM_PATH);
}

const allTargetsDir = path.join(__dirname, "npm", `vocana-cli`);
const allTargetsBinDir = path.join(allTargetsDir, "bin");
fs.mkdirSync(allTargetsDir)
fs.mkdirSync(allTargetsBinDir)
// generate package.json
fs.writeFileSync(
    path.join(allTargetsDir, "package.json"),
    JSON.stringify(
        {
            name: `@vocana/vocana-cli`,
            version: version,
            description: `All-platform cli binaries for @vocana/vocana`,
            preferUnplugged: true,
            engines: {
                node: ">=12",
            },
            publishConfig: {
                registry: "https://npm.pkg.github.com",
            },
        },
        null,
        2
    )
);
// generate README.md
fs.writeFileSync(
    path.join(allTargetsDir, "README.md"),
    `# \`@vocana/vocana-cli\`\n\nThis is all-platform cli binaries for @vocana/vocana`
);
fs.writeFileSync(
    path.join(allTargetsDir, "index.js"),
    genIndexJs()
);

for (const target of targets) {
    const targetDir = path.join(__dirname, "npm", `vocana-cli-${target}`);
    const binDir = path.join(targetDir, "bin");
    // build rust for each target
    const result = spawnSync("cargo", ["build", "--release", "--target", target], {
        stdio: "inherit",
        env: {
            ...process.env,

            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER: "x86_64-linux-gnu-gcc",
            CC_x86_64_unknown_linux_gnu: "x86_64-linux-gnu-gcc",
            CXX_x86_64_unknown_linux_gnu: "x86_64-linux-gnu-g++",
            AR_x86_64_unknown_linux_gnu: "x86_64-linux-gnu-ar",

            CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER: "aarch64-linux-gnu-gcc",
            CC_AARCH64_unknown_linux_gnu: "aarch64-linux-gnu-gcc",
            CXX_AARCH64_unknown_linux_gnu: "aarch64-linux-gnu-g++",
            AR_AARCH64_unknown_linux_gnu: "aarch64-linux-gnu-ar",
        }
    });
    if (result.status !== 0) {
        process.exit(result.status);
    }
    // generate npm directory
    fs.mkdirSync(targetDir);
    fs.mkdirSync(binDir);
    // copy target artifacts
    fs.copyFileSync(
        path.join(__dirname, "target", target, "release", "vocana"),
        path.join(binDir, "vocana")
    );
    // copy target artifacts to all-platform directory
    fs.copyFileSync(
        path.join(__dirname, "target", target, "release", "vocana"),
        path.join(allTargetsBinDir, target)
    );
    // generate package.json
    fs.writeFileSync(
        path.join(targetDir, "package.json"),
        genPackageJson(target)
    );
    // generate README.md
    fs.writeFileSync(path.join(targetDir, "README.md"), genReadme(target));
    // generate index.js
    fs.writeFileSync(
        path.join(targetDir, "index.js"),
        genIndexJs()
    );
}

function genPackageJson(target) {
    return JSON.stringify(
        {
            name: `@vocana/vocana-cli-${target}`,
            version: version,
            description: `The ${target} cli binary for @vocana/vocana`,
            os: [targetConfig[target].os],
            cpu: [targetConfig[target].cpu],
            preferUnplugged: true,
            engines: {
                node: ">=12",
            },
            publishConfig: {
                registry: "https://npm.pkg.github.com",
            },
        },
        null,
        2
    );
}

function genReadme(target) {
    return `# \`@vocana/vocana-cli-${target}\`\n\nThis is the ${target} cli binary for @vocana/vocana`;
}

function getVersion() {
    const toml = fs.readFileSync(path.join(__dirname, "Cargo.toml"), "utf8");
    const version = toml.match(/version = "(.*)?"/)[1];
    return version;
}

function genIndexJs() {
    return `throw new Error("Do not require me");`
}