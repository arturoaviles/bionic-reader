export default {
  serverExternalPackages: ['@arturoaviles/bionic-reader'],
  webpack(config) {
    config.experiments = { ...config.experiments, topLevelAwait: true };
    config.output.environment = { ...config.output.environment, asyncFunction: true };
    return config;
  },
};
