import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

export const __dirname = path.dirname(fileURLToPath(import.meta.url));

export async function getVersion() {

    const toml = await fs.promises.readFile(path.join(path.dirname(__dirname), "Cargo.toml"), "utf8");
    const version = toml.match(/version = "(.*)?"/)[1];
    return version;
}