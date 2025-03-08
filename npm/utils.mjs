import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

export const __dirname = path.dirname(fileURLToPath(import.meta.url));

/**
 * @param {string} url
 * @param {string} outputPath
 * @returns {Promise<void>}
 */
export async function downloadFile(url, outputPath) {
    try {
        console.log(`start downloading: ${url}`);
        const directory = path.dirname(outputPath);
        await fs.promises.mkdir(directory, { recursive: true });
        const response = await fetch(url);

        if (!response.ok) {
            throw new Error(`download fail: ${response.status} ${response.statusText}`);
        }

        const fileStream = fs.createWriteStream(outputPath);
        const arrayBuffer = await response.arrayBuffer();
        fileStream.write(Buffer.from(arrayBuffer));

        return new Promise((resolve, reject) => {
            fileStream.on('finish', () => {
                console.log(`file save to: ${outputPath}`);
                resolve();
            });
            fileStream.on('error', (err) => {
                reject(err);
            });
            fileStream.end();
        });
    } catch (error) {
        console.error(`download fail: ${error.message}`);
        throw error;
    }
}

export async function getVersion() {

    const toml = await fs.promises.readFile(path.join(path.dirname(__dirname), "Cargo.toml"), "utf8");
    const version = toml.match(/version = "(.*)?"/)[1];
    return version;
}