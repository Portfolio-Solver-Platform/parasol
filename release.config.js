module.exports = {
  branches: ['main'],
  plugins: [
    '@semantic-release/commit-analyzer',
    '@semantic-release/release-notes-generator',

    ['@semantic-release/exec', {
      prepareCmd: './scripts/release/prepare ${nextRelease.version}',
      publishCmd: './scripts/release/publish ${nextRelease.version}'
    }],

    '@semantic-release/github'
  ]
};
