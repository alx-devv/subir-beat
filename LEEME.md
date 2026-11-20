# Subir Beat

App de Windows para subir un beat de punta a punta. Cargás los datos y los archivos, apretás **Publicar beat**, y la app:

1. Arma los zips Premium (WAV) y Exclusiva (WAV + stems) con tu nomenclatura.
2. Sube el mp3 con tag y los zips a `beats-entregables`, y verifica que estén completos.
3. Si el visual es un video, genera el gif liviano para la tienda.
4. Crea el producto en Shopify con las dos licencias, los metacampos (BPM, key, género, mood, instrumentos, preview) y los links de descarga.
5. Lo publica en todos los canales y verifica que haya quedado activo con los dos links.
6. En paralelo, renderiza el video de YouTube (1920×1080, visual cuadrado de 1080 al centro, franjas negras, loop hasta el final del beat).
7. Deja listos el título, la descripción y los tags, y adapta tu miniatura a 1280×720 si hace falta.

Si algo falla (por ejemplo, se corta internet), el beat queda en **Historial** y se retoma desde el paso que falló, sin duplicar archivos ni productos.

---

## Instalar

El instalador se compila solo en GitHub:

1. Creá un repositorio privado en GitHub (por ejemplo `subir-beat`).
2. Subí todo el contenido de esta carpeta (incluida la carpeta `.github`).
3. Entrá a la pestaña **Actions** del repo: la compilación arranca sola y tarda unos 10 minutos.
4. Cuando termine, abrí la ejecución y descargá **Subir-Beat-instalador**. Adentro está el `.exe`.
5. Instalalo. ffmpeg ya viene incluido.

Windows puede mostrar el aviso de SmartScreen porque el instalador no está firmado: **Más información → Ejecutar de todas formas**.

---

## Configurar (una sola vez)

Abrí **Ajustes** en la app.

### Shopify

1. Entrá a <https://dev.shopify.com/dashboard> con tu cuenta de la tienda.
2. **Create app** → nombre: `Subir Beat`.
3. En la versión de la app, en **Access scopes**, marcá:
   `write_products`, `read_products`, `write_publications`, `read_publications`, `write_files`, `read_files`.
4. Guardá la versión e **instalá la app en tu tienda** (`alexgareis`).
5. En **Settings** de la app copiá el **Client ID** y el **Client secret** y pegalos en Ajustes.

El dominio myshopify ya viene cargado (`pc1swd-9c.myshopify.com`).

### Cloudflare R2

1. Cloudflare → **R2** → **Manage R2 API Tokens** → **Create API token**.
2. Permiso: **Object Read & Write**, limitado al bucket `beats-entregables`.
3. Copiá el **Access Key ID** y el **Secret Access Key**.
4. El **Account ID** está en la página principal de R2.

Los dos secretos se guardan en el Administrador de credenciales de Windows, no en un archivo.

Después tocá **Probar conexiones**: tienen que aparecer los tres puntos en verde.

---

## Usar

- Arrastrá todo junto a la ventana: la app reconoce sola el WAV, el mp3, la carpeta de stems, el visual y la miniatura (si es 16:9). También podés soltar cada cosa en su lugar o hacer clic para elegirla.
- El formulario se guarda solo mientras lo completás.
- Al terminar, **Abrir YouTube Studio** copia el título, abre la carpeta del video y abre Studio. Los botones **Copiar** tienen la descripción y los tags.

Los videos quedan en `Videos\Beats subidos\<nombre del beat>` junto con `youtube.txt` y la miniatura.

---

## YouTube automático (más adelante)

Hoy la subida a YouTube es semiautomática. Para que la app suba sola, Google tiene que aprobar la auditoría de la API de YouTube del proyecto. Cuando esté aprobada, se agrega la subida directa sin cambiar nada del resto.

---

## Para desarrollo

```
npm install
npx tauri dev
```

Necesita Rust, Node 22 y `ffmpeg` en el PATH (o en `src-tauri/binaries`).
