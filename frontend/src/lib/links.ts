// Внешние ссылки студии: репозиторий, ишью, лицензии, автор и поддержка.
const REPO = "https://github.com/timoncool/dub-studio";

export const LINKS = {
  repo: REPO,
  issues: `${REPO}/issues`,
  releases: `${REPO}/releases`,
  license: `${REPO}/blob/master/LICENSE`,
  modelLicenses: `${REPO}#license`,
};

export const DONATE = {
  boosty: "https://boosty.to/neuro_art",
  dalink: "https://dalink.to/nerual_dreming",
  github: REPO,
  telegram: "https://t.me/nerual_dreming",
  crypto: [["BTC", "1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC"],
           ["ETH · ERC20", "0xb5db65adf478983186d4897ba92fe2c25c594a0c"],
           ["USDT · TRC20", "TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C"]] as const,
};
