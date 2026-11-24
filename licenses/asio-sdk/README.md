# Steinberg ASIO SDK 2.3.4 (2025-10-15)

## 🇵🇱 Polski

Steinberg ASIO SDK 2.3.4 jest używany zgodnie z opcją GPLv3; zachowano oryginalne noty Steinberg i pełne źródła składników użytych przez asio-sys. ZIP źródłowy SDK jest załączany do wydania. Nazwa ASIO jest znakiem Steinberg, bez certyfikacji sprzętowej. Sterownik producenta lub ASIO4ALL trzeba zainstalować osobno w architekturze zgodnej z aplikacją.

## 🇬🇧 English

Copyright (c) 2025 Steinberg Media Technologies GmbH. This distribution chooses the SDK's **GPL version 3** option, not the proprietary license alternative. Original notices are retained. GPLv3/AGPLv3 combination is permitted by their section 13; Prospekt's application code remains AGPL-3.0-or-later. ASIO is a Steinberg trademark; the application does not bundle or display the ASIO logo and is not hardware-certified by Steinberg.

Official download: https://download.steinberg.net/sdk_downloads/ASIO-SDK_2.3.4_2025-10-15.zip

Official archive SHA-256: `d5ebf0c20dd2c5f43771fd0c1418f4b361bf52434ee670097cfa6b3a335e2eca`.

The Windows build script `scripts/prepare_asio_sdk.py` verifies this digest and selects the unmodified `common` and `host` sources used by asio-sys. These sources, original notices and the GPL text are provided as `ASIO-SDK-2.3.4-GPL3-source.zip` alongside the Windows binary in Releases. No hardware driver installer is included: users install a compatible vendor driver matching the application architecture (32/64-bit) separately. Realtek can use WASAPI or a separately installed ASIO4ALL driver.
