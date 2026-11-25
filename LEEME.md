# Subir Beat

## Novedades de la versión 2

- Diseño nuevo: tres columnas, fondo cálido, Archivo y acento azul medianoche.
- Vista previa en vivo de cómo queda en YouTube (búsqueda y página del video) y en la tienda.
- Al soltar el WAV, completa nombre, BPM y tonalidad desde el nombre del archivo (`@alexgareis_nombre_94bpm_C#m.wav`) y los compara con el análisis del audio.
- Cada zip lleva un `Léeme.txt` (uno para Premium y otro para Exclusiva), editable en Ajustes.
- Al instalarla encima de la versión 1 se conservan las claves y los ajustes.

App de Windows para subir un beat de punta a punta. Cargás los datos y los archivos, apretás **Publicar beat**, y la app:

1. Arma los zips Premium (WAV) y Exclusiva (WAV + stems) con tu nomenclatura.
2. Sube el mp3 con tag y los zips a `beats-entregables`, y verifica que estén completos.
3. Sube la portada (el GIF original; si el visual es un video, genera el gif) y una miniatura WebP animada de 240 px para el catálogo, dentro de la carpeta del beat en R2.
4. Da de alta el beat en alexgareis.com (Cloudflare): título, BPM, tonalidad, géneros, moods, instrumentos, portada, miniatura, mp3, zips y la onda del reproductor. Queda publicado al instante y sumado al borrador de la app beatstore, así ninguna publicación lo borra.
5. En paralelo, renderiza el video de YouTube (1920×1080, visual cuadrado de 1080 al centro, franjas negras, loop hasta el final del beat).
6. Deja listos el título, la descripción y los tags, y adapta tu miniatura a 1280×720 si hace falta.

Si algo falla (por ejemplo, se corta internet), el beat queda en **Historial** y se retoma desde el paso que falló, sin duplicar archivos ni beats.

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

### Tienda

Subir Beat usa la misma clave que la app beatstore. Si la app beatstore está instalada en esta compu, la toma sola. Si no, pegala en **Ajustes → Tienda → Clave de la app**.

Los precios de las licencias salen de la tienda (se cambian en la app beatstore → Producto).

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

## Versión 3

**YouTube.** En Ajustes → YouTube cargá el ID y el secreto de cliente (están en el JSON que descargaste de Google Cloud, en `client_id` y `client_secret`), guardá y tocá "Conectar con YouTube". Se abre el navegador: elegí la cuenta del canal y aceptá. Mientras la app de Google esté en modo prueba, Google muestra un aviso de "app no verificada": tocá "Continuar". Desde ahí cada beat sube el video solo, con la miniatura y la visibilidad que elijas en el formulario. Hasta que Google apruebe la auditoría, YouTube deja los videos en privado; cuando la aprueben, marcá la casilla correspondiente en Ajustes.

La conexión de YouTube en modo prueba vence a los 7 días: si la app avisa que se venció, volvé a tocar "Conectar con YouTube". Cuando Google apruebe la app, deja de vencer.

**Actualizaciones.** Al abrir la app se busca una versión nueva; también está el botón "Buscar actualizaciones" en Ajustes → Versión. Las versiones se publican en GitHub Releases, firmadas: la clave privada vive en el secreto `TAURI_SIGNING_PRIVATE_KEY` del repositorio. Si esa clave se pierde, las versiones nuevas no se pueden instalar desde la app y hay que reinstalar a mano.

## Versión 3.1: mapa de artistas

- **Notion:** creá una integración interna en https://www.notion.so/profile/integrations, copiá su clave y pegala en Ajustes → Mapa de artistas. Después, en la base "Contactos de la landing", menú ⋯ → Conexiones → agregá la integración. Sin ese paso Notion no deja leer la base.
- **Búsqueda de YouTube:** usa una clave de API de un proyecto de Google aparte (no el de la subida de videos), con la YouTube Data API v3 habilitada. Cada artista medido gasta unas 200 unidades de las 10.000 diarias; las mediciones se guardan 14 días.
