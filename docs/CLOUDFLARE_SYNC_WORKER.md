# Guía de Despliegue en la Nube: Cloudflare Workers + D1 (`notium-db`)

Esta arquitectura se alinea con la implementación actual de **Cloud Sync v1** de Notium (`server/cloudflare-sync`), utilizando exclusivamente **Cloudflare Workers** y **Cloudflare D1** dentro de la capa gratuita (sin Cloudflare R2 ni Durable Objects).

---

## 1. Componentes de la arquitectura

| Componente Cloudflare | Rol en Notium | Características |
|---|---|---|
| **Cloudflare Workers** | Enrutado HTTP (`/sync/health`, `/sync/push`, `/sync/pull`, `/sync/blob/:hash`) y autenticación | Validación obligatoria de `Authorization: Bearer <AUTH_TOKEN>` (comparación en tiempo constante), normalización de `vault_id` y verificación de rutas de adjuntos (`attachments/*`). |
| **Cloudflare D1 (`notium-db`)** | Almacenamiento SQLite serverless de operaciones (`journal`) y adjuntos binarios (`blobs`) | Almacena el historial HLC de notas en `journal` y los binarios de adjuntos (`<= 900.000 bytes`) directamente en `blobs (data BLOB)`, particionados por `vault_id`. |

---

## 2. Archivo de Configuración (`server/cloudflare-sync/wrangler.jsonc`)

```jsonc
{
	"$schema": "node_modules/wrangler/config-schema.json",
	"name": "notium-sync",
	"main": "src/index.ts",
	"compatibility_date": "2024-12-01",
	"d1_databases": [
		{
			"binding": "DB",
			"database_name": "notium-db",
			"database_id": "<id que imprime wrangler d1 create>"
		}
	]
}
```

Configura el secreto de autenticación (**obligatorio**):
```bash
npx wrangler secret put AUTH_TOKEN
```

Sin `AUTH_TOKEN` el Worker rechaza todas las peticiones con `503`, para no exponer el Vault en una URL pública. Solo para desarrollo local (`wrangler dev`) puedes permitir el acceso sin token definiendo `ALLOW_UNAUTHENTICATED=true` en un archivo `.dev.vars`.

---

## 3. Esquema SQL de D1 (`server/cloudflare-sync/schema.sql`)

```sql
CREATE TABLE IF NOT EXISTS journal (
  vault_id TEXT NOT NULL DEFAULT 'default',
  op_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  note_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  base_rev INTEGER NOT NULL,
  new_rev INTEGER NOT NULL,
  content BLOB,
  hlc_ts INTEGER NOT NULL,
  hlc_cnt INTEGER NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY (vault_id, op_id)
);

CREATE TABLE IF NOT EXISTS blobs (
  vault_id TEXT NOT NULL DEFAULT 'default',
  hash TEXT NOT NULL,
  path TEXT NOT NULL,
  size INTEGER NOT NULL DEFAULT 0,
  data BLOB NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY (vault_id, hash, path)
);

CREATE INDEX IF NOT EXISTS idx_journal_vault_ts ON journal(vault_id, ts);
CREATE INDEX IF NOT EXISTS idx_blobs_vault_ts ON blobs(vault_id, ts);
```

> El Worker ejecuta además `ensureSchema(env.DB)` de forma idempotente en arranque para crear la tabla `blobs`, migrar `vault_id` en `journal` si procede de una versión previa y asegurar los índices.

---

## 4. Endpoints del Worker (`server/cloudflare-sync/src/index.ts`)

- `GET /sync/health` → `{ "status": "ok", "engine": "cloudflare-d1" }`
- `POST /sync/push` → Recibe `{ vault_id, device_id, ops }` e inserta en lotes de hasta 100 sentencias en `journal`.
- `GET /sync/pull?since=<cursor>&vault_id=<vault_id>` → Devuelve `{ cursor, ops, blobs }` filtrando `journal` y `blobs` por `vault_id` y `ts > since`.
- `POST /sync/blob/:hash?vault_id=<vault_id>&path=<attachments/...>` → Valida la ruta y el tamaño (`<= 900.000 bytes`) y persiste el binario en `blobs.data` en D1.
- `GET /sync/blob/:hash?vault_id=<vault_id>` → Lee `data` desde `blobs` en D1 y lo devuelve como `application/octet-stream`.

---

## 5. Despliegue y conexión en Notium

1. Despliega el Worker desde `server/cloudflare-sync`:
   ```bash
   cd server/cloudflare-sync
   npx wrangler d1 execute notium-db --remote --file=./schema.sql
   npx wrangler deploy
   ```
2. Abre Notium en PC o Android y ve a **Ajustes > Sincronización en la Nube**:
   - **URL del Servidor**: `https://notium-sync.<tu-subdominio>.workers.dev` (se exige `https://`; `http://` solo se acepta para `localhost`)
   - **Token de Autenticación**: El valor configurado en `AUTH_TOKEN`.
   - **ID del Vault (partición en D1)**: Identificador `[a-z0-9_-]{1,64}` (por defecto `default`). Usa el mismo ID en los dispositivos que compartan el mismo Vault.
   - Haz clic en **"Guardar configuración"** y en **"Sincronizar ahora"**.

---

## 6. Notium Web y Cloudflare Access

La versión web ([`docs/WEB.md`](WEB.md)) es un Worker aparte, `notium-web` (`server/notium-web`):

- Sirve `dist-web/` como archivos estáticos.
- Reenvía `/sync/*` a `notium-sync` mediante un *service binding*. Es una llamada interna que no pasa por Access, y el token Bearer sigue siendo obligatorio.

`notium-sync` solo expone la API.

```bash
npm run cloud:deploy:web   # build:web + wrangler deploy de server/notium-web
```

**Access:** en **Workers & Pages → notium-web → Settings → Domains & Routes**, activa Cloudflare Access en la fila de `workers.dev` y deja en la política (**Allow**) solo tu email.

No actives Access en `notium-sync`: el PC y el móvil llaman a `/sync/*` sin navegador y recibirían la redirección de login. Además, Access no permite añadir una excepción por ruta en un `workers.dev` que ya protege otra aplicación (`destination belongs to another application`).

## 7. Papelera compartida

- Las ops de borrado definitivo llevan `purge: true` (columna `journal.purge`). Al recibirlas, el Worker borra todo el contenido de esa nota y conserva solo sus ops de borrado.
- Un cron diario (`triggers.crons` en `wrangler.jsonc`, a las 03:17 UTC) borra el historial de las notas cuya última op es un borrado de hace más de 30 días.

Para probar el cron en local:

```bash
npx wrangler dev --test-scheduled
curl "http://127.0.0.1:8787/__scheduled?cron=17+3+*+*+*"
```

## 8. Columna `path` del journal

Las ops de crear, renombrar o mover llevan un `path` opcional (ruta del archivo). El Worker la guarda en `journal.path` (añadida con un `ALTER TABLE` idempotente en `ensureSchema`) y solo la devuelve en `/sync/pull` cuando existe, así que las ops antiguas no cambian.
