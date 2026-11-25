# Política de privacidad de Conversor de stickers

Última actualización: 24 de noviembre de 2026.

Conversor de stickers es una app de código abierto que convierte imágenes y vídeos en stickers para
WhatsApp y en GIF, emojis y stickers para Discord. Esta política explica qué datos trata la app.
Resumen: **no tiene cuentas, no muestra anuncios y no rastrea a nadie.**

## Lo que no hacemos

- No hay registro ni cuentas de usuario.
- No hay anuncios ni SDK publicitarios.
- No hay analíticas, rastreo ni identificadores de publicidad.
- No vendemos ni compartimos datos personales, porque no los recogemos.

## Tus archivos

Las imágenes, vídeos y stickers que eliges, pegas o compartes con la app se procesan **en tu
dispositivo**. No se suben a ningún servidor nuestro. Los stickers y paquetes que creas se guardan en
el almacenamiento privado de la app y se borran al desinstalarla. Si compartes un sticker o lo añades
a WhatsApp, lo recibe la app que elijas, bajo la política de esa app.

## Conexiones a internet

La app solo se conecta a internet para estas funciones:

- **Búsqueda de GIF y stickers (KLIPY y GIPHY).** Al buscar o ver tendencias, la app envía el texto
  de la búsqueda y la página pedida. Las peticiones a KLIPY pasan por un proxy nuestro alojado en
  Cloudflare Workers, que añade la clave de la API para que no viaje dentro de la app; el proxy no
  guarda ni registra la dirección IP, las búsquedas ni ninguna otra información, y solo aplica un límite
  de peticiones por minuto. GIPHY solo se usa si tú introduces tu propia clave, y entonces la petición va
  directamente a GIPHY. KLIPY y GIPHY pueden tratar los datos técnicos de la petición (como la dirección
  IP) según sus propias políticas de privacidad.
- **Catálogo de la comunidad.** La app descarga el índice y los paquetes de stickers desde un sitio
  estático público (GitHub Pages). Se aplican las condiciones de GitHub a esa conexión.
- **Enlaces directos a imágenes o vídeos.** Si pegas un enlace https que apunta a un archivo, la app
  descarga ese archivo desde el servidor del enlace.

La edición completa de la app, distribuida fuera de Google Play, también puede descargar vídeos de
enlaces de otros sitios; la edición de Google Play no incluye esa función.

## Permisos de Android

- **Internet:** para las conexiones descritas arriba.
- **Acceso a archivos:** la app no pide permisos de almacenamiento. Los archivos se abren con el
  selector del sistema, que solo da acceso a lo que eliges, o llegan cuando los compartes con la app.
- **Consulta de WhatsApp instalado:** la app comprueba si WhatsApp o WhatsApp Business están
  instalados para ofrecer el botón de añadir paquetes. No lee datos de WhatsApp.

## Menores

La app no recoge datos de nadie, tampoco de menores.

## Cambios y contacto

Si esta política cambia, se actualizará este documento y su fecha. Para dudas o solicitudes, abre una
incidencia en <https://github.com/Duk3lo/conversor-stickers/issues>.

Esta política está publicada en
<https://github.com/Duk3lo/conversor-stickers/blob/main/docs/privacidad.md>.
