// Azure canary login smoke through GitHub OAuth. Optional beside the Entra
// smoke: it runs only when SELF_HOSTED_GITHUB_* credentials are configured.
const {
  clickFirstVisible,
  decodeBase32,
  generateTotp,
  hasReturnedToApp,
  runLoginSmoke,
  runTotpSelfTest,
} = require("./login-smoke-common");

const email = process.env.G5_GITHUB_EMAIL;
const password = process.env.G5_GITHUB_PASSWORD;
const totpSecret = process.env.G5_GITHUB_TOTP_SECRET || "";

async function loginWithGitHub(page, appOrigin) {
  const githubLink = page.getByRole("link", { name: /continue with github/i });
  await githubLink.waitFor({ state: "visible", timeout: 30000 });
  await githubLink.click();
  const deadline = Date.now() + 120000;
  while (Date.now() < deadline) {
    if (hasReturnedToApp(page, appOrigin)) return;
    // Only enter test credentials on GitHub's origin.
    if (new URL(page.url()).origin === "https://github.com") {
      const login = page.locator('input[name="login"]');
      if (await login.isVisible().catch(() => false)) {
        await login.fill(process.env.G5_GITHUB_USER || email);
        await page.locator('input[name="password"]').fill(password);
        await page.getByRole("button", { name: /^sign in$/i }).click();
        continue;
      }
      const otp = page.locator('input[name="app_otp"]');
      if (totpSecret && await otp.isVisible().catch(() => false)) {
        await otp.fill(generateTotp(totpSecret));
        await page.getByRole("button", { name: /^verify$/i }).click();
        continue;
      }
      await clickFirstVisible(page, [{ name: /^authorize /i }], 1500);
    }
    await page.waitForURL(url => url.origin === appOrigin && !url.pathname.includes("/login"), {
      timeout: 1500, waitUntil: "domcontentloaded",
    }).catch(() => {});
  }
  throw new Error(`Timed out waiting for GitHub login to return to ${appOrigin}; current URL: ${page.url()}`);
}

async function main() {
  if (!email || !password) {
    throw new Error("G5_GITHUB_EMAIL and G5_GITHUB_PASSWORD are required");
  }
  await runLoginSmoke({
    providerLabel: "GitHub",
    email,
    password,
    login: loginWithGitHub,
    screenshotPrefix: "github-login",
  });
}

function runSelfTest() {
  runTotpSelfTest();
}

if (require.main === module) {
  if (process.env.G5_GITHUB_SMOKE_SELF_TEST === "1") {
    runSelfTest();
  } else {
    main().catch((error) => {
      console.error(error);
      process.exit(1);
    });
  }
}

module.exports = {
  decodeBase32,
  generateTotp,
  loginWithGitHub,
  runSelfTest,
};
