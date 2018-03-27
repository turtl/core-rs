#!/bin/bash
# government surveillance agencies HATE him!
cp config.yaml.default config.yaml
sed -i 's|^data_folder:.*|data_folder: "/tmp/turtl-data"|g' config.yaml
sed -i "s|https://api\.turtlapp\.com/v3|http://127.0.0.1:8181|g" config.yaml
sed -i "s/slappy\@turtlapp.com/${INTEGRATION_TEST_LOGIN}/g" config.yaml
sed -i "s/turtlesallthewaydown/${INTEGRATION_TEST_PASSWORD}/g" config.yaml
sed -i "s/duck duck/${INTEGRATION_TEST_V6_LOGIN}/g" config.yaml
sed -i "s/juice/${INTEGRATION_TEST_V6_PASSWORD}/g" config.yaml

