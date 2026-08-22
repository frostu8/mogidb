# MogiDB
The `gutbuster` API database.

## Authorization setup
MogiDB will allow all requests by default. To secure the endpoints, generate an
access token, then set the `ACCESS_TOKEN` environment variable with the
generated value.

```bash
echo "ACCESS_TOKEN=$(openssl rand -hex 32)" >> .env
```
