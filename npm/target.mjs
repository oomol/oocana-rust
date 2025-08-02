export const packageTemplate = {
    "engines": {
        "node": ">=20"
    },
    "files": [
        "bin", "index.js"
    ],
    "main": "index.js",
    "publishConfig": {
        "registry": "https://npm.pkg.github.com"
    }
}

export function readme(target) {
    return `# \`@oomol/oocana-cli-${target}\`\n\nThis is the ${target} cli binary for @oomol/oocana`;
}

export const targetConfig = {
    "x86_64-apple-darwin": {
        os: ["darwin"],
        cpu: ["x64"],

    },
    "aarch64-apple-darwin": {
        os: ["darwin"],
        cpu: ["arm64"],
    },
    "x86_64-unknown-linux-gnu": {
        os: ["linux"],
        cpu: ["x64"],
    },
    "aarch64-unknown-linux-gnu": {
        os: ["linux"],
        cpu: ["arm64"],
    },
    // "x86_64-unknown-linux-musl": {
    //     os: "linux",
    //     cpu: "x64",
    // }
};