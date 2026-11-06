# Supervision

## Sondes simples

- `GET /healthz` (l'API répond) et `GET /readyz` (l'API et PostgreSQL répondent) sur le domaine de l'API : à surveiller avec
  Uptime Kuma ou équivalent, avec une alerte au bout de 2 échecs.
- LiveKit : `GET /` sur son domaine répond « OK ».

## Métriques Prometheus

Renseigner `METRICS_TOKEN` (une chaîne aléatoire) : l'API expose alors `GET /metrics` en Bearer. Sans jeton, la route répond 404.

```yaml
scrape_configs:
  - job_name: estompe
    scheme: https
    metrics_path: /metrics
    authorization: { credentials: "<METRICS_TOKEN>" }
    static_configs: [{ targets: ["chat.plbls.fr"] }]
```

| Métrique | Sens |
| --- | --- |
| `estompe_up`, `estompe_database_up` | l'API et PostgreSQL répondent (1) ou non (0) |
| `estompe_storage_bytes`, `estompe_storage_max_bytes` | fichiers stockés et plafond |
| `estompe_ws_users`, `estompe_ws_connections` | personnes connectées en temps réel |
| `estompe_users`, `estompe_messages`, `estompe_files` | volumes |
| `estompe_http_requests_total{code}` | requêtes par classe de code |
| `estompe_http_request_duration_seconds_{sum,count}` | durée moyenne des requêtes |

## Alertes suggérées

```yaml
groups:
  - name: estompe
    rules:
      - alert: EstompeInjoignable
        expr: up{job="estompe"} == 0 or estompe_database_up == 0
        for: 2m
      - alert: EstompeStockagePresquePlein
        expr: estompe_storage_max_bytes > 0 and estompe_storage_bytes / estompe_storage_max_bytes > 0.9
        for: 30m
        annotations: { summary: "Les plus anciens fichiers vont être supprimés" }
      - alert: EstompeErreursServeur
        expr: rate(estompe_http_requests_total{code="5xx"}[5m]) > 0.1
        for: 10m
      - alert: EstompeLent
        expr: rate(estompe_http_request_duration_seconds_sum[5m]) / rate(estompe_http_request_duration_seconds_count[5m]) > 0.5
        for: 10m
```

Disque du VPS, mémoire et processeur : `node_exporter` ou les graphes de Dokploy. Pour le vocal, les métriques de LiveKit
(`prometheus_port` dans sa configuration) donnent le nombre de salles et de participants.

## Runner CI (act_runner)

À faire sur le VPS (EST-3, lots 1 et 6) :

- Version : `act_runner --version`, mettre à jour si elle est ancienne (la sauvegarde du cache en dépend).
- Cache : dans `config.yaml` du runner, `cache.enabled: true`, `cache.dir` sur le volume persistant, et si la sauvegarde échoue
  (connexion refusée, délai dépassé) fixer `cache.host` (IP du runner joignable depuis les conteneurs) et `cache.port`, publier ce port
  et l'autoriser dans le pare-feu pour le réseau Docker.
- Charge : surveiller CPU et mémoire du VPS quand les workflows backend et frontend tournent ensemble ; ajuster `runner.capacity`.
- Disque : un cache Rust pèse 1 à 3 Go par version de `Cargo.lock`. Alerte à 80 % d'occupation, et purge hebdomadaire, par exemple
  (cron) : `find <cache.dir> -type f -mtime +14 -delete && docker system prune -af --filter until=168h`.
