// Azure canary login smoke through Microsoft Entra ID. This is the required
// login proof for the self-hosted product: enterprise customers sign in with
// Entra, so the canary must complete a real tenant login before it provisions
// the worker host and runs the microVM chat turn.
const {
  clickFirstVisible,
  decodeBase32,
  generateTotp,
  hasReturnedToApp,
  runLoginSmoke,
  runTotpSelfTest,
  saveDiagnosticScreenshot,
} = require("./login-smoke-common");

const email = process.env.G5_ENTRA_EMAIL;
const password = process.env.G5_ENTRA_PASSWORD;
const totpSecret = process.env.G5_ENTRA_TOTP_SECRET || "";

function isMicrosoftLoginUrl(rawUrl) {
  try {
    const url = new URL(rawUrl);
    if (url.protocol !== "https:") return false;
    // Match on a label boundary so look-alike hosts never receive credentials.
    return ["microsoftonline.com", "microsoft.com", "live.com"].some(
      (domain) => url.hostname === domain || url.hostname.endsWith(`.${domain}`),
    );
  } catch {
    return false;
  }
}

function isMicrosoftSecurityRegistrationUrl(rawUrl) {
  try {
    const url = new URL(rawUrl);
    return (
      url.hostname === "mysignins.microsoft.com" &&
      (url.pathname.includes("/register") ||
        url.pathname.includes("/security-info") ||
        url.searchParams.has("requirement"))
    );
  } catch {
    return false;
  }
}

async function fillVisibleOrThrow(page, selector, value, label) {
  const locator = page.locator(selector).first();
  await locator.waitFor({ state: "visible", timeout: 45000 }).catch(() => {
    throw new Error(`Expected visible ${label} field on ${page.url()}`);
  });
  await locator.fill(value);
  return locator;
}

async function clickVisibleOrThrow(page, candidates, label) {
  const clicked = await clickFirstVisible(page, candidates, 30000);
  if (!clicked) {
    throw new Error(`Expected visible ${label} control on ${page.url()}`);
  }
}

async function chooseMicrosoftWorkAccountIfRequested(page) {
  return clickFirstVisible(
    page,
    [
      { text: /^Work or school account$/i },
      { selector: '[data-test-id="aadTile"], [data-test-id="workOrSchoolAccount"]' },
    ],
    1500,
  );
}

async function submitMicrosoftPasswordIfRequested(page) {
  const field = page
    .locator('input[name="passwd"], input[type="password"], #i0118')
    .first();
  if (!(await field.isVisible({ timeout: 500 }).catch(() => false))) {
    return false;
  }
  await field.fill(password);
  const clicked = await clickFirstVisible(
    page,
    [
      { name: /sign in/i },
      { selector: '#idSIButton9, input[type="submit"][value="Sign in"], input[type="submit"]' },
    ],
    5000,
  );
  if (!clicked) {
    await field.press("Enter");
  }
  return true;
}

async function fillMicrosoftTotpIfRequested(page) {
  if (!totpSecret) {
    return false;
  }
  const field = page
    .locator(
      [
        "#idTxtBx_SAOTCC_OTC",
        'input[name="otc"]',
        'input[name="code"]',
        'input[autocomplete="one-time-code"]',
        'input[inputmode="numeric"]',
        'input[type="tel"]',
      ].join(", "),
    )
    .first();
  if (!(await field.isVisible({ timeout: 500 }).catch(() => false))) {
    return false;
  }
  await field.fill(generateTotp(totpSecret));
  await clickVisibleOrThrow(
    page,
    [
      { name: /verify/i },
      { name: /continue/i },
      { name: /next/i },
      { selector: '#idSubmit_SAOTCC_Continue, #idSIButton9, input[type="submit"]' },
    ],
    "Microsoft MFA verification",
  );
  return true;
}

async function driveMicrosoftSecurityRegistrationOrThrow(page, firstSeenAtMs) {
  const clickedEscape = await clickFirstVisible(
    page,
    [
      { role: "button", name: /skip(?: setup| for now| this)?/i },
      { role: "link", name: /skip(?: setup| for now| this)?/i },
      { role: "button", name: /ask later/i },
      { role: "link", name: /ask later/i },
      { role: "button", name: /not now/i },
      { role: "link", name: /not now/i },
      { role: "button", name: /remind me later/i },
      { role: "link", name: /remind me later/i },
      { role: "button", name: /maybe later/i },
      { role: "link", name: /maybe later/i },
      { role: "button", name: /cancel/i },
      { role: "link", name: /cancel/i },
      { selector: '#idBtn_Back, #cancelLink, [data-testid="cancelButton"]' },
    ],
    2500,
  );
  if (clickedEscape) {
    return;
  }

  if (Date.now() - firstSeenAtMs < 20000) {
    return;
  }

  await saveDiagnosticScreenshot(page, "entra-security-registration-required.png");
  throw new Error(
    [
      `Microsoft Entra required security/app registration before returning to G5: ${page.url()}.`,
      "The Azure canary browser cannot complete a mandatory Microsoft Authenticator enrollment from CI.",
      "Exempt the self-hosted-azure-canary test identity from the registration campaign/Conditional Access requirement,",
      "or pre-register an MFA method and set AZURE_ENTRA_TEST_USER_TOTP_SECRET so the smoke can answer MFA code prompts.",
    ].join(" "),
  );
}

async function waitForAppReturnOrDriveMicrosoftPrompts(page, appOrigin) {
  const deadline = Date.now() + 120000;
  const appReturn = (url) => url.origin === appOrigin && !url.pathname.includes("/login");
  let securityRegistrationFirstSeenAt = null;
  // A rejected password or code brings its prompt back; resubmitting it until
  // the deadline would trip Entra smart lockout on the shared canary account.
  let passwordSubmissions = 0;
  let totpSubmissions = 0;
  while (Date.now() < deadline) {
    if (hasReturnedToApp(page, appOrigin)) {
      return;
    }
    const returned = await page
      .waitForURL(appReturn, { timeout: 1500, waitUntil: "domcontentloaded" })
      .then(() => true)
      .catch(() => false);
    if (returned || hasReturnedToApp(page, appOrigin)) {
      return;
    }

    // Only interact with Microsoft-owned pages; never type credentials into
    // anything else that may appear on the way back to the app.
    if (!isMicrosoftLoginUrl(page.url())) {
      continue;
    }

    if (await chooseMicrosoftWorkAccountIfRequested(page)) {
      continue;
    }

    if (await submitMicrosoftPasswordIfRequested(page)) {
      passwordSubmissions += 1;
      if (passwordSubmissions > 1) {
        await saveDiagnosticScreenshot(page, "entra-password-rejected.png");
        throw new Error(`Microsoft rejected the Entra test password at ${page.url()}`);
      }
      continue;
    }

    if (await fillMicrosoftTotpIfRequested(page)) {
      totpSubmissions += 1;
      if (totpSubmissions > 2) {
        await saveDiagnosticScreenshot(page, "entra-totp-rejected.png");
        throw new Error(`Microsoft rejected the Entra TOTP code at ${page.url()}`);
      }
      continue;
    }

    if (isMicrosoftSecurityRegistrationUrl(page.url())) {
      securityRegistrationFirstSeenAt = securityRegistrationFirstSeenAt || Date.now();
      await driveMicrosoftSecurityRegistrationOrThrow(page, securityRegistrationFirstSeenAt);
      continue;
    }

    // "Stay signed in?", consent and similar interstitials.
    await clickFirstVisible(
      page,
      [
        { name: /accept/i },
        { name: /^yes$/i },
        { name: /continue/i },
        {
          selector: [
            "#idSIButton9",
            'input[type="submit"][value="Accept"]',
            'input[type="submit"][value="Yes"]',
            'input[type="submit"][value="Continue"]',
            'input[type="submit"]',
          ].join(", "),
        },
      ],
      1500,
    );
  }
  throw new Error(
    `Timed out waiting for Microsoft login to return to G5 at ${appOrigin}; current URL: ${page.url()}`,
  );
}

async function loginWithMicrosoft(page, appOrigin) {
  const microsoftLink = page.getByRole("link", { name: /continue with microsoft/i });
  await microsoftLink.waitFor({ state: "visible", timeout: 30000 });
  await microsoftLink.click();

  await page.waitForURL((url) => url.hostname.endsWith("microsoftonline.com"), {
    timeout: 45000,
  });
  await fillVisibleOrThrow(
    page,
    'input[name="loginfmt"], input[type="email"], #i0116',
    email,
    "Microsoft email",
  );
  await clickVisibleOrThrow(
    page,
    [
      { name: /next/i },
      { selector: '#idSIButton9, input[type="submit"][value="Next"], input[type="submit"]' },
    ],
    "Next",
  );
  await waitForAppReturnOrDriveMicrosoftPrompts(page, appOrigin);
}

async function main() {
  if (!email || !password) {
    throw new Error("G5_ENTRA_EMAIL and G5_ENTRA_PASSWORD are required");
  }
  await runLoginSmoke({
    providerLabel: "Entra",
    email,
    password,
    login: loginWithMicrosoft,
    screenshotPrefix: "entra-login",
  });
}

function runSelfTest() {
  runTotpSelfTest();
  if (!isMicrosoftSecurityRegistrationUrl("https://mysignins.microsoft.com/register?requirement=App")) {
    throw new Error("security-registration URL self-test failed");
  }
  for (const lookAlike of [
    "https://evil.example.com/login.microsoftonline.com",
    "https://evilmicrosoft.com/",
    "https://attackermicrosoftonline.com/",
    "http://login.microsoftonline.com/",
  ]) {
    if (isMicrosoftLoginUrl(lookAlike)) {
      throw new Error(`Microsoft origin self-test failed for ${lookAlike}`);
    }
  }
  if (!isMicrosoftLoginUrl("https://login.microsoftonline.com/common/oauth2/v2.0/authorize")) {
    throw new Error("Microsoft origin self-test failed");
  }
}

if (require.main === module) {
  if (process.env.G5_ENTRA_SMOKE_SELF_TEST === "1") {
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
  isMicrosoftSecurityRegistrationUrl,
  loginWithMicrosoft,
  runSelfTest,
};
