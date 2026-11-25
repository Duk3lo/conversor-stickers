# Ficha de Google Play (edición Play)

Textos y respuestas sugeridas para Play Console. La edición Play se compila con
`app/src-tauri/android-play.sh` (sin descargas por enlace con yt-dlp, icono violeta). El icono de
512 px para la ficha es `app/src-tauri/icons/icono-play-512.png`.

## Datos de la app

- ID de aplicación: `es.conversorstickers.app`
- Categoría: Herramientas (alternativa: Fotografía). Etiqueta: aplicación, no juego.
- Política de privacidad: https://github.com/Duk3lo/conversor-stickers/blob/main/docs/privacidad.md
- Contacto: incidencias de GitHub (https://github.com/Duk3lo/conversor-stickers/issues). Play exige
  además un correo de contacto en la ficha: usa uno que no quieras mantener privado.

## Nombre (máx. 30)

`Conversor de Stickers` (21 caracteres; no incluye «WhatsApp»).

## Descripción breve (máx. 80)

`Crea stickers animados y GIF a partir de tus vídeos e imágenes.` (63 caracteres)

## Descripción completa (máx. 4000)

```
Conversor de Stickers convierte tus vídeos e imágenes en stickers animados y GIF, sin cuentas, sin
anuncios y sin rastreo.

QUÉ PUEDES HACER
• Elige un vídeo o una imagen de tu galería, o pégalo desde otra app, y conviértelo en un sticker
  animado o estático.
• Recorta el tramo que quieras, ajusta la velocidad y los fotogramas por segundo, y encuadra la imagen.
• Si el clip es demasiado largo o pesado, la app lo ajusta para que cumpla los límites de tamaño y
  duración.
• Crea paquetes de stickers, ponles icono y guárdalos en tu móvil.
• Añade tus paquetes a WhatsApp: los stickers que crea la app están pensados para usarse como stickers
  para WhatsApp.
• Genera también GIF, emojis animados y stickers para Discord, con las reglas de cada formato.
• Busca GIF y stickers en KLIPY y GIPHY y úsalos como base de tus propios stickers.
• Explora el catálogo de la comunidad y descarga paquetes de otras personas.
• Comparte tus stickers con cualquier app o guárdalos como archivo.

PRIVACIDAD
Tus archivos se procesan en tu dispositivo y no se suben a ningún servidor. La app no tiene cuentas, no
muestra anuncios y no rastrea. Solo se conecta a internet para la búsqueda de GIF, el catálogo de la
comunidad y los enlaces directos a archivos que pegues tú. No pide permisos de almacenamiento: los
archivos se abren con el selector del sistema.

SOFTWARE LIBRE
La app es de código abierto: https://github.com/Duk3lo/conversor-stickers

AVISO
Esta aplicación es independiente y no está afiliada, respaldada ni patrocinada por WhatsApp ni por Meta
Platforms, Inc. «WhatsApp» es una marca de sus propietarios y se menciona solo para indicar
compatibilidad. Tampoco está afiliada a Discord, KLIPY ni GIPHY. Eres responsable de tener derecho a
usar y transformar los vídeos e imágenes con los que creas stickers.
```

## Contenido gráfico

- Icono: `icono-play-512.png` (512 x 512 PNG).
- Gráfico de funciones (1024 x 500) y al menos 2 capturas de teléfono (hacen falta 4 para ciertas
  funciones destacadas): haz las capturas de la edición Play (sin campo de enlaces) y evita que se
  vea el logotipo de WhatsApp.
- No uses el verde y la forma de WhatsApp en ningún gráfico: el icono violeta existe para evitar la
  confusión de marca.

## Seguridad de los datos

Respuestas sugeridas (revísalas contra el código antes de enviar):

- ¿La app recoge o comparte datos de usuario obligatorios? **No recoge datos personales.**
  Hay conexiones de red (búsqueda de GIF, catálogo, enlaces directos), pero el texto de las búsquedas
  no se asocia a una persona ni lo guarda el desarrollador.
- Si Play Console lo trata como «datos compartidos» por la búsqueda: **Actividad en la app > Historial
  de búsqueda** enviado a KLIPY (proveedor de servicio), temporal, **no obligatorio** para usar la app,
  sin vincular a la identidad, finalidad «Funcionalidad de la app». La dirección IP la ve KLIPY o
  GIPHY al responder; declárala como «Información del dispositivo o de otro tipo» solo si Play lo pide.
- ¿Todos los datos se cifran en tránsito? **Sí** (solo https).
- ¿Puedes solicitar que se eliminen los datos? **No aplica**: no hay cuentas ni datos guardados
  por el desarrollador. Al desinstalar se borran los archivos locales.
- Fotos y vídeos: se procesan solo en el dispositivo, no se recogen ni se comparten.
- Ubicación, contactos, mensajes, identificadores de publicidad, pagos, salud: **no**.

## Clasificación de contenido (IARC)

Cuestionario de «Utilidad / productividad / otros» (no es red social ni juego):

- Violencia, contenido sexual, lenguaje soez, drogas, apuestas: **No**.
- Contenido generado por usuarios o interacción entre usuarios: **No** (no hay cuentas ni chat; el
  catálogo de la comunidad es un sitio estático moderado fuera de la app).
- Comparte ubicación, permite compras digitales, accede a internet sin restricción: **No** a las dos
  primeras. Búsqueda de GIF de terceros con filtro de contenido fijo: indícalo si el cuestionario
  pregunta por contenido de terceros.
- Resultado esperado: PEGI 3 / Para todos o similar. Si KLIPY o GIPHY cuentan como contenido de
  terceros no controlado, puede subir a «Para mayores de 12».

## Público objetivo y contenido

- Grupo de edad objetivo: **13 años o más** (o 18+ si prefieres evitar los requisitos del programa
  Familias). No marques menores de 13: la app no está dirigida a niños y la búsqueda de GIF de terceros
  no está pensada para ese público.
- ¿Atrae involuntariamente a menores? Los stickers atraen a todas las edades; responde con honestidad
  que podría haber usuarios jóvenes, sin que la app esté dirigida a ellos.
- Anuncios: **No contiene anuncios**.
- Acceso a la app sin restricciones: **Sí** (no hay inicio de sesión).
- Declaración de permisos: solo INTERNET; no hay permisos sensibles.
- Aplicación de noticias, salud, finanzas, gobierno: **No**.

## Prueba cerrada: 12 testers durante 14 días

Las cuentas personales nuevas deben hacer una prueba cerrada con al menos 12 testers que se mantengan
suscritos 14 días seguidos antes de pedir el acceso a producción.

1. Play Console > la app > **Pruebas > Prueba cerrada** > crea una pista (por ejemplo «Amigos»).
2. En **Testers**, crea una lista de correos (Gmail o cuentas de Google) o un Grupo de Google con al
   menos 12 personas (mejor 15-20, por si alguien se da de baja). Guarda la lista.
3. Sube el AAB de `~/Escritorio/play/ConversorStickers-1.0.0-play.aab` en **Crear versión nueva**, con
   las notas de la versión en español, y envíala a revisión. La primera revisión puede tardar días.
4. Copia el **enlace de participación** de la pista y mándalo a los testers. Cada uno tiene que abrirlo,
   pulsar «Hacerse tester» e instalar la app desde Play Store (con la misma cuenta de la lista).
5. Pídeles que la mantengan instalada y la abran de vez en cuando; el contador de 14 días solo cuenta
   si siguen suscritos. Vigila en **Panel** que se mantengan al menos 12.
6. Recoge comentarios (puedes usar las incidencias de GitHub) y, si corriges algo, sube una versión nueva
   a la misma pista sin reiniciar el contador.
7. Pasados los 14 días con 12 testers, Play Console habilita **Solicitar acceso a producción**:
   responde el cuestionario (cómo se hizo la prueba, qué comentarios hubo y qué cambiaste) y envíalo.
8. Cuando lo aprueben, crea la versión de producción con el mismo AAB (o uno posterior, con un
   `versionCode` mayor).

Antes de subir: completa Contenido de la app (política de privacidad, anuncios, acceso, clasificación,
público objetivo, seguridad de los datos) y activa Play App Signing; la clave del llavero es la clave de
subida.
