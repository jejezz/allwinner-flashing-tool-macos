# SD카드 카드부팅(카드 퓨징) 조사

**목적**: PhoenixCard(Windows 전용, 오류 빈발)를 대체하기 위해 `aw-tool`에 SD카드를 직접 "카드 퓨징(卡量产)" 이미지로 만드는 서브커맨드(`flash-sdcard`)를 추가하기 위한 실물 SDK 기반 조사.

**조사 대상 SDK**: `Z:\Work\android15\aosp\android15` (u-boot: `longan\brandy\brandy-2.0\u-boot-2018`, tools: `longan\tools\tools_win`)

**조사 방식**: 소스 코드(grep/read) + SDK에 포함된 벤더 매뉴얼(`启动量产2.0手册.docx`, docx 내부 XML을 직접 파싱해서 추출) 분석. 실기 테스트는 하지 않았다.

---

## 0. 핵심 발견 요약

1. **카드부팅(卡量产/card product)은 FEL/EFEX(USB)와 완전히 다른 경로**다. USB가 전혀 필요 없다 — SD카드를 보드에 꽂고 전원만 넣으면, 보드의 BootROM이 카드의 boot0를 읽어 `work_mode=0x11`(`WORK_MODE_CARD_PRODUCT`)을 보고 "카드에서 읽어 내장 eMMC/flash에 자가 설치"하는 루틴(`sunxi_card_sprite_main()`)으로 분기한다. ([cmd/sunxi_sprite.c:40](cmd/sunxi_sprite.c:40))
2. **카드 위에 올라가는 파일은 aw-tool이 이미 다루는 것과 완전히 같은 IMAGEWTY 컨테이너(firmware.img)** 다. `sprite_card_firmware_probe()`가 `Img_Open(name)`으로 이 파일을 그대로 열어 안의 `sys_partition.fex`/`boot0`/`uboot`/파티션 항목을 읽는다. 즉 **새 이미지 포맷을 만들 필요가 없다** — 기존 FEL용 firmware.img를 그대로 카드에 올리면 된다. ([sprite/sprite_card.c:89-98](sprite/sprite_card.c:89))
3. **물리 레이아웃(섹터/LBA)이 전부 고정값**으로 소스와 벤더 매뉴얼 양쪽에서 확인됨:
   - boot0: LBA **16** (`BOOT0_SDMMC_START_ADDR`)
   - uboot/TOC1(boot_package): LBA **32800**, 백업 LBA **24576**
   - firmware.img(IMAGEWTY 전체를 raw로) 시작: LBA **40960** (= 20 MiB, `logical_start`)
4. **work_mode 패치만 다르면 된다**: FEL/EFEX용은 boot0 work_mode를 `0x10`(USB_PRODUCT)으로 패치하지만, 카드 퓨징용은 **원본 그대로인 `0x11`(CARD_PRODUCT)을 유지**해야 한다. 체크섬 알고리즘은 동일(`mksunxiboot.c: gen_check_sum()`) — **aw-tool의 `src/sunxi_head.rs`에 이미 `WORK_MODE_CARD_PRODUCT = 0x11`과 `set_work_mode`/`fix_checksum`이 구현되어 있어 바로 재사용 가능**하다. ([sunxi_head.rs:34](../src/sunxi_head.rs:34))
5. **진행 상태 LED 표시 기능이 u-boot에 이미 구현되어 있다** (`sprite_led_init/turn/exit`, `CONFIG_SUNXI_SPRITE_LED`). 정상 동작 중엔 느리게(기본 500ms) 깜빡이고, 에러 시 빠르게(기본 100ms) 깜빡인다. GPIO 핀과 깜빡임 주기는 **u-boot 소스 수정이 아니라 sys_config/DTS의 `card_boot` 노드 설정값**이다. 단, 이 기능을 켜는 `CONFIG_SUNXI_SPRITE_LED`는 **T527/T507 현재 defconfig에서 꺼져 있어(`n`) u-boot 재빌드가 필요**하다 (6절 참조).
6. **`tools_win`에는 소스가 없다** — PhoenixCard/PhoenixSuit는 전부 벤더 바이너리(`PhoenixCard_V4.1.9_20190227.zip` 등)뿐이고, 재현에 쓸 소스는 없다. 다만 `Documentations\启动量产2.0手册.docx`에 카드 퓨징 레이아웃과 LED 설정법이 사람이 읽을 수 있는 문서로 설명되어 있어 소스 분석을 그대로 뒷받침한다.
7. **FEL/EFEX와 달리 호스트-보드 간 프로토콜이 없다** — aw-tool이 할 일은 "올바른 바이트를 올바른 섹터에 쓰는 것"뿐이다(USB/EFEX 핸드셰이크, FEL 부트스트랩 전혀 불필요). 구현 난이도는 FEL/EFEX보다 훨씬 낮다.

---

## 0.5 정정과 실물 검증 (구현 후 추가)

구현하면서 **실제 SDK 이미지와, PhoenixCard가 구운 실제 카드의 덤프**로 이 문서의 가정을 확인했다. 아래가 소스 분석만으로 적었던 내용과 달라진 부분이다.

| 항목 | 이 문서의 처음 설명 | 실물로 확인된 것 |
|---|---|---|
| 카드에 써지는 boot0 | `work_mode`를 `0x11`로 패치 + 체크섬 재계산 | 맞다 — PhoenixCard의 boot0와 **바이트 단위로 동일**(offset 224와 체크섬 12바이트만 다름) |
| boot_package(TOC1) | 수정 없이 복사 | 맞다 — 카드 위 값이 이미지와 0바이트 차이 |
| boot0 체크섬 범위 | `fix_checksum`을 그대로 재사용 | PhoenixCard 카드의 체크섬은 **앞 48바이트(헤더)** 합이다. SDK 원본 boot0는 전체 61440바이트 합. BootROM이 어느 쪽을 보는지는 미확정 |
| boot0 헤더 `length` | (오프셋 구분 없음) | boot0는 **offset 16**, u-boot 헤더는 offset 20 |
| "BootROM이 boot0의 work_mode를 보고 분기" (1절) | — | 정확하지 않다. work_mode는 u-boot 헤더 안(`uboot_spare_head.boot_data.work_mode`)에서 읽히고, **boot0(SPL)가 `get_card_work_mode()`로 그 값을 채워 넣는다**(`spl-pub/nboot/main/boot0_main.c:209`의 `update_uboot_info`). `get_card_work_mode()` 본체는 미공개 라이브러리라 읽는 위치는 소스로 확인하지 못했고, 카드의 실물이 offset 224임을 보여 줄 뿐이다 |

중간에 "work_mode가 boot0가 아니라 u-boot에 있으니 `boot_package`의 u-boot를 패치해야 한다"는 가설을 세워 구현했다가 되돌렸다. 실제 카드가 그렇지 않음을 보여 줬기 때문이다 — **소스를 읽은 추론보다 실물 한 장이 강한 증거였다.**

벤더 도구에 대한 단서(이미지 안의 `cardtool.fex` 문자열): `card_work_mode=0x%x`, `use default card_work_mode`, `uboot magic is error`, `Magic Complete` / `Magic Failed`. 또 `cardscript.fex`는 레이아웃(boot0 → 16, `BOOTPKG` → 32800, `[card_boot] start = 40960`)을 그대로 확인해 준다.

**여전히 확인하지 못한 것**: 이 도구로 쓴 카드를 보드에 꽂았을 때 설치가 끝나는지(보드 필요), BootROM의 boot0 체크섬 기준, `get_card_work_mode()`가 offset 224를 읽는 조건.

---

## 0.6 실기에서 재현된 실패와 정정 (2026-10-29)

0.5절은 **틀렸다.** 실기(UART)로 확인한 순서:

1. boot0를 패치해 쓴 카드는 보드가 쳐다보지도 않고 eMMC로 부팅했다(`card no is 2`, 8bit). 원인은 boot0의 체크섬이었다 — `card_boot0()`가 u-boot 헤더용 `length` offset(20)을 boot0에 써서 전체 61440바이트가 아니라 앞 48바이트만 합산했다. 이 boot0는 BootROM이 거부한다. PhoenixCard 카드와 원본 `boot0_sdcard.fex`를 61440바이트 전체로 비교하면 차이가 0이다: boot0는 **패치하지 않는다.** (`src/sdcard/verify.rs`는 이미 `BOOT0_LENGTH_OFFSET = 16`이라고 적어 놓고 쓰지 않았다.)
2. 패치를 없애자 BootROM이 SD카드(`card no is 0`, 4bit)에서 boot0와 boot_package를 올렸지만, u-boot가 `workmode = 0`(PhoenixCard 카드는 17)으로 일반 부팅을 했다. 카드 모드는 **boot0가 아니라 u-boot가 정한다.**
3. 두 카드를 섹터 단위로 비교하니 차이가 이것뿐이었다:
   - 본체 boot_package(LBA 32800): u-boot 헤더 offset 224가 `0 → 0x11`, u-boot 헤더 체크섬 +0x11, TOC1 헤더 체크섬 +0x22 (4바이트). 백업(LBA 24576)은 이미지 그대로.
   - LBA 40960: PhoenixCard는 firmware가 아니라 이미지의 `sunxi_mbr.fex`(128섹터)를 쓴다.
   - firmware(IMAGEWTY)는 LBA 40960이 아니라 **LBA 98304**에 있다(u-boot 로그 `img start = 0x18000`, `read mmc start 98304`). 우리가 40960으로 알았던 `logical_start`는 파티션 테이블 위치였다.
   - PhoenixCard는 그 밖에 GPT와 `thisdata.fex`(FAT 이미지)도 쓰는데, 이 도구는 쓰지 않는다 (필요 여부는 보드에서 확인할 것).

0.5절에서 "boot0는 offset 224와 체크섬만 다르고 boot_package는 0바이트 차이"라고 적은 비교는 PhoenixCard 카드가 아니라 이 도구의 옛 출력을 본 것으로 보인다. `offset 224 = work_mode`는 u-boot 헤더 기준으로 맞는 값이었고(`docs/T527-T507-FEL-EFEX-기술조사.md:142`), 그걸 boot0에 적용한 게 틀렸다.

**수정**: boot0는 원본 그대로, 본체 boot_package에는 u-boot work mode `0x11`(증분 체크섬), LBA 40960에 `sunxi_mbr.fex`, firmware는 LBA 98304. 구현 결과는 PhoenixCard 카드의 boot0·파티션 테이블·boot_package 두 슬롯과 바이트 단위로 같다.

---

## 1. work_mode 전체 표

출처: `include/spare_head.h:13-27`

| 값 | 이름 | 의미 |
|---|---|---|
| `0x00` | `WORK_MODE_BOOT` | 정상 부팅 |
| `0x10` | `WORK_MODE_USB_PRODUCT` | USB(FEL/EFEX) 양산 모드 — aw-tool이 지금 패치하는 값 |
| `0x11` | `WORK_MODE_CARD_PRODUCT` | **카드 양산(퓨징) 모드** — 이번 조사 대상 |
| `0x12` | `WORK_MODE_USB_DEBUG` | USB EFEX 프로토콜 기반 디버그 모드 |
| `0x13` | `WORK_MODE_SPRITE_RECOVERY` | 내장 백업 파티션에서 복구 |
| `0x14` | `WORK_MODE_CARD_UPDATE` | SD카드에서 펌웨어 자동 업데이트(= 이미 부팅된 시스템 쪽 로직, 카드 퓨징과는 다름) |
| `0x15` | `WORK_MODE_UDISK_UPDATE` | USB 메모리에서 자동 업데이트 |
| `0x20` | `WORK_MODE_USB_UPDATE` | — |
| `0x21` | `WORK_MODE_OUTER_UPDATE` | — |

`WORK_MODE_PRODUCT = (1<<4) = 0x10`, `WORK_MODE_UPDATE = (1<<5) = 0x20` 두 비트 마스크로 위 값들이 조합되어 있다(0x11 = 0x10 | 0x01, 0x14 = 0x10 | 0x04 로 보이는 패턴은 아니고 각 값이 독립 상수로 정의돼 있음 — 비트마스크는 `env/sunxi_flash.c`에서 `workmode & WORK_MODE_PRODUCT` 식으로 상위 분류에만 쓰임).

aw-tool은 이미 `WORK_MODE_BOOT/USB_PRODUCT/CARD_PRODUCT/USB_DEBUG` 4개를 `src/sunxi_head.rs:32-35`에 상수로 가지고 있다.

---

## 2. 카드부팅 분기 로직

### 2.1 BootROM → work_mode 분기 (`cmd/sunxi_sprite.c`)

```c
// cmd/sunxi_sprite.c:21-50
int do_sprite_test(...)
{
    printf("sunxi work mode=0x%x\n", get_boot_work_mode());
    if (get_boot_work_mode() == WORK_MODE_USB_PRODUCT) {
        ... // FEL/EFEX 경로 (기존 조사에서 이미 확인)
    }
#ifdef CONFIG_SUNXI_SDMMC
    else if (get_boot_work_mode() == WORK_MODE_CARD_PRODUCT) {
        printf("run card sprite\n");
#ifdef CONFIG_SUNXI_SPRITE_LED
        sprite_led_init();          // ← 진행 LED 점멸 시작
#endif
        ret = sunxi_card_sprite_main(0, NULL);   // ← 카드 퓨징 메인 루프
#ifdef CONFIG_SUNXI_SPRITE_LED
        sprite_led_exit(ret);        // ← ret<0이면 에러 패턴으로 전환
#endif
        return ret;
    }
#endif
    ...
}
```

`WORK_MODE_CARD_PRODUCT`(0x11)일 때만 `sunxi_card_sprite_main()`이 호출된다. `WORK_MODE_CARD_UPDATE`(0x14, 66-86행)는 전혀 다른 함수(`auto_update_check`)로 가는데, 이는 **이미 정상 부팅된 시스템이 SD카드에서 업데이트 파일을 찾아 자동 업데이트**하는 경로이고(이름이 헷갈리기 쉽다), 우리가 원하는 "맨 카드로 벽돌 상태 보드를 되살리는" 경로는 `CARD_PRODUCT`다.

### 2.2 카드 퓨징 본체 (`sprite/sprite_card.c`)

- `sprite_card_firmware_probe(name)` — `Img_Open()`으로 카드 위의 IMAGEWTY 파일을 연다. `name`은 `sunxi_card_sprite_main()`이 내부적으로 넘기는 경로/오프셋 문자열. ([sprite_card.c:89-98](../))
- `sprite_card_firmware_start()` → `sunxi_partition_get_offset(1)` — 카드 위에서 firmware.img가 시작하는 섹터(= 40960, 3절 참조).
- `sprite_card_fetch_download_map()` / `sprite_card_fetch_mbr()` — IMAGEWTY 안의 `"1234567890DLINFO"`, `"1234567890___MBR"` 아이템을 읽어 다운로드 맵·MBR을 얻는다 — **FEL/EFEX 쪽 EFEX 서버가 호스트에서 하던 일을, 카드 퓨징에서는 보드 자신의 u-boot가 그대로 재사용**한다.
- `sunxi_sprite_deal_boot0()` / `sunxi_sprite_deal_uboot()` — IMAGEWTY에서 `"1234567890BOOT_0"`(boot0) · `"UBOOT_0000000000"`/`"BOOTPKG-00000000"`(uboot/TOC1) 아이템을 읽어, **보드 자신의 내장 eMMC/flash**에 쓴다(`sunxi_sprite_download_boot0/uboot`).
- `sunxi_sprite_deal_part()` → `__download_normal_part()` — `sys_partition.fex` 기반 각 파티션을 IMAGEWTY에서 읽어 내장 flash에 sparse/raw 그대로 write.

즉 **카드 위의 firmware.img는 FEL/EFEX로 쓸 때와 1바이트도 다를 필요가 없는 동일 파일**이고, "누가 그 파일을 해석해서 내장 flash에 쓰느냐"만 다르다(FEL=호스트의 aw-tool, 카드=보드 자신의 u-boot).

---

## 3. SD카드 물리 레이아웃

출처: `include/spare_head.h:60-67` + 벤더 매뉴얼(`启动量产2.0手册.docx`, "卡量产" 절, `sys_config1.fex` 예시) + `arch/arm/dts/sun55iw3p1-soc-system.dts:93-98`

| 영역 | 섹터(LBA, 512B 단위) | 바이트 오프셋 | 비고 |
|---|---|---|---|
| boot0 (eGON, `boot0_sdcard.fex`) | **16** | 8,192 | `BOOT0_SDMMC_START_ADDR` |
| uboot/TOC1 (`boot_package.fex`) 백업 | 24,576 | 12,582,912 (12 MiB) | `UBOOT_BACKUP_START_SECTOR_IN_SDMMC` |
| uboot/TOC1 (`boot_package.fex`) 메인 | **32,800** | 16,793,600 | `UBOOT_START_SECTOR_IN_SDMMC` |
| firmware.img (IMAGEWTY 전체, raw copy) | **40,960** | 20,971,520 (20 MiB) | DTS `card_boot` 노드의 `logical_start = <40960>` |

벤더 매뉴얼에 나온 구식 `sys_config1.fex` 표기(이 값이 최신 DTS의 `card_boot` 노드로 그대로 이어짐):

```ini
[card_boot]
logical_start      = 40960
sprite_gpio0       = port:ph2<1><-1><-1><1>
sprite_work_delay  = 1000
sprite_err_delay   = 100
```

최신 DTS 표기(`arch/arm/dts/sun55iw3p1-soc-system.dts:93-98`):

```dts
card_boot:card_boot@45000014 {
    device_type = "card_boot";
    logical_start   = <40960>;
    /* sprite_gpio0 = <&pio PC 7 1 0xffffffff 0xffffffff 1>; */
    sprite_gpio0 = <&pio 0x2 0x7 0x1 0xffffffff 0xffffffff 0x1>;
};
```

**boot0/uboot 사이에 24,576~32,800 섹터, uboot/firmware 사이에 그 이후 공간은 예비(reserved) 영역**으로, 특별히 채우지 않아도 된다(0으로 두거나 비워도 무방 — 매뉴얼에 "덮어쓰지 않으면 원래 데이터가 남아있어도 무방"하다는 설명 있음, 단 실기로 재확인 필요).

→ **구현해야 할 것은 결국**: `(1) boot0_sdcard.fex를 work_mode=0x11로 패치 후 LBA16에 write, (2) boot_package.fex(TOC1)를 LBA32800·LBA24576에 write, (3) firmware.img 전체를 LBA40960부터 raw로 write` 세 가지뿐이다. GPT/MBR을 카드에 새로 만들 필요가 없다 — 보드의 BootROM/SPL은 파티션 테이블을 보지 않고 고정 LBA만 읽는다.

---

## 4. 체크섬/서명

- `mksunxiboot.c: gen_check_sum()` — FEL/EFEX 조사에서 이미 확인한 것과 동일한 32비트 워드섬 알고리즘. 암호화 서명이 아니다.
- boot0 구조체의 `check_sum` 필드(offset 12)는 **work_mode를 바꾸면(offset 224) 반드시 재계산**해야 한다 — 이것은 FEL용 패치와 완전히 동일한 메커니즘이고, **aw-tool의 `fix_checksum()`을 그대로 재사용**할 수 있다(`src/sunxi_head.rs:57-86`).
- TOC1(`boot_package.fex`)는 `toc1->add_sum` 필드로 별도 체크섬을 갖지만(`sprite_card.c:1092` `sunxi_sprite_verify_checksum(buffer, toc1->valid_len, toc1->add_sum)`), **카드 퓨징에서는 TOC1 자체를 수정하지 않고 그대로 복사**하므로 재계산이 필요 없다 — work_mode는 boot0에만 있다.

---

## 5. `tools_win` 확인 결과

`Z:\Work\android15\aosp\android15\longan\tools\tools_win`에는 소스코드가 없다. 전부 벤더 배포 바이너리/압축 아카이브:

```
PhoenixCard_V4.1.9_20190227.zip   ← SD카드 퓨징 GUI 본체(바이너리)
PhoenixSuit_CN.msi / _debug.msi   ← USB 퓨징 GUI
USBDriver*.zip, UDISK_drivers.zip ← 드라이버
Documentations/
  启动量产2.0手册.doc / .docx      ← "부팅 양산 2.0" 매뉴얼 (사람이 읽는 문서, 소스 아님)
```

`启动量产2.0手册.docx`는 Read 도구로 바이너리라 직접 못 열어서, docx(zip) 안의 `word/document.xml`을 `unzip`으로 꺼내 `<w:t>` 텍스트 노드만 추출하는 방식으로 내용을 확인했다. 이 매뉴얼의 "卡量产"(카드 퓨징), "量产过程中的LED指示方式"(퓨징 중 LED 표시 방식) 절이 3절·6절 내용의 근거다. 이미지(스크린샷)는 추출하지 않았다 — 필요하면 `word/media/imageN.png`에서 꺼낼 수 있다.

→ **PhoenixCard를 재현하는 데 필요한 모든 로직은 이미 u-boot 소스(보드 쪽)에 있고, PhoenixCard가 하는 일은 "그 레이아웃대로 호스트에서 raw write"뿐**임을 문서로도 재확인했다. 벤더 바이너리를 역공학할 필요가 없다.

---

## 6. (추가 조사) 퓨징 진행 LED/GPIO 표시

사용자 요구사항: SD카드로 퓨징하는 동안 USB/디스플레이 없이도 보드에 붙은 LED로 시작/성공/실패를 구분해서 보고 싶다.

### 6.1 이미 구현되어 있다 — 소스 패치가 아니라 "설정"이다

`sprite/sprite_led.c` 전체가 이 기능이다:

- `sprite_led_init()` ([sprite_led.c:77-117](../)) — `sunxi_card_sprite_main()` 진입 직후(`cmd/sunxi_sprite.c:43`에서 호출) GPIO를 잡고, 타이머로 **기본 500ms 주기 토글**을 시작한다. 주기는 DTS `card_boot` 노드의 `sprite_work_delay` 프로퍼티로 바꿀 수 있다.
- `sprite_led_exit(status)` ([sprite_led.c:160-188](../)) — `sunxi_card_sprite_main()`이 끝난 뒤(`cmd/sunxi_sprite.c:47`) 호출. **`status < 0`(실패)이면 타이머를 더 빠른 주기(기본 100ms, `sprite_err_delay`)로 재설정**해서 계속 깜빡이게 한다. 성공 시에는 단순히 타이머를 멈춘다(`del_timer`) — 즉 "성공 = LED 꺼짐/정지", "실패 = 빠르게 계속 깜빡임", "진행 중 = 느리게 깜빡임" 3단계가 이미 구현돼 있다.
- GPIO 핀은 `fdt_get_one_gpio(FDT_PATH_CARD_BOOT, "sprite_gpio0", &gpio_init)`로 **DTB에서 런타임에 읽는다** (`FDT_PATH_CARD_BOOT = "/soc/card_boot"`, `include/sys_config.h:35`). 즉 **u-boot C 소스를 고칠 필요가 없다** — 어떤 GPIO를 쓸지는 100% 보드 DTS/설정의 몫이다.

### 6.2 후킹 지점 (요청사항 1번)

| 시점 | 위치 | 함수 |
|---|---|---|
| 퓨징 시작 | `cmd/sunxi_sprite.c:43` (`sunxi_card_sprite_main()` 호출 직전) | `sprite_led_init()` |
| 메인 루프 | `sprite/sprite_card.c` 전체 (파티션 단위로 진행) | `sunxi_card_sprite_main()` → `sunxi_sprite_deal_boot0/uboot/part()` |
| 종료(성공/실패) | `cmd/sunxi_sprite.c:47` (`sunxi_card_sprite_main()` 리턴 직후) | `sprite_led_exit(ret)` |

이미 세 지점 모두 소스에 존재하므로 **새로 후킹할 필요가 없다** — `CONFIG_SUNXI_SPRITE_LED`를 켜고 GPIO를 설정하기만 하면 된다.

### 6.3 기존 GPIO/LED 관련 코드 (요청사항 2번)

- `drivers/gpio/` 안의 범용 GPIO 드라이버(`sunxi_gpio_request()` 등)를 그대로 사용한다. 전용 LED 드라이버가 아니라 "임의의 GPIO를 켰다 껐다 하는" 범용 로직(`sprite_timer_func()` → `gpio_write_one_pin_value()`)이다.
- **같은 SoC(`sun55iw3p1`, T527)의 기본 레퍼런스 DTS에 이미 활성화된 예시가 있다**: `arch/arm/dts/sun55iw3p1-soc-system.dts:97` — `sprite_gpio0 = <&pio 0x2 0x7 0x1 0xffffffff 0xffffffff 0x1>;` (포트 PC, 핀 7, 출력 모드). 반면 T507(`sun50iw9p1-soc-system.dts:211`)은 주석으로만 남아있고 실제 핀이 지정돼 있지 않다.
- 이 값들은 SoC 레퍼런스 보드용이라, **실제 pluto 보드(T527/T507 커스텀 보드)가 PC7/PH6를 LED나 다른 용도로 이미 쓰고 있는지는 이 SDK만으로 확인 불가** — 보드 DTS(이 u-boot 트리에는 없었고, android 쪽 커널 DTS나 보드 스키매틱에 있을 것)를 찾아야 한다.

### 6.4 GPIO 핀 출처 (요청사항 3번)

핀 번호는 다음 중 하나에서 가져와야 하며, 범위 안에서는 "SoC 레퍼런스 DTS의 예시 값을 그대로 쓸 수 있는지"는 확인했지만 "pluto 보드에서 실제로 비어있는 핀인지"는 **조사 범위 밖(실기/보드 스키매틱 확인 필요)**:

1. 보드 DTS (안드로이드 커널 쪽, `device/` 또는 커널 트리의 `sun55iw3-pluto*.dts` 류 — 이번 조사에서 u-boot 트리 안에만 머물러서 못 찾음)
2. 보드 사양서/스키매틱의 여분 GPIO
3. 최후 수단: 기존에 안 쓰는 범용 GPIO 헤더 핀을 하드코딩

> **보드별 DTS 위치 (SDK 전체 검색 결과)**: 보드 단위 오버라이드는 `longan/device/config/chips/<soc>/configs/<board>/uboot-board.dts`에 있다 (T507: `janus` / `rhea` / `titan`, `longan/device/product/configs/<board>/`에도 사본). 이 SDK에는 `pluto*` 보드 디렉터리가 보이지 않으므로, 실제 pluto 보드의 `uboot-board.dts`는 별도 SDK/브랜치에 있을 가능성이 높다. 전체 트리에서 `sprite_gpio0`를 실제 값으로 지정한 파일은 SoC 레퍼런스 DTS뿐이다.

### 6.5 변경 범위 — u-boot 재빌드가 필요한가? (요청사항 4, 5번)

**결론: u-boot 재빌드가 필요하다. aw-tool의 런타임 바이너리 패치만으로는 불가능하다.**

이유:
- `CONFIG_SUNXI_SPRITE_LED`는 **Kconfig 컴파일 타임 옵션**이다(`sprite/Kconfig:36-40`, `default n`). 이 옵션이 꺼진 채로 컴파일된 boot0/u-boot 바이너리에는 `sprite_led_init/turn/exit` 호출 자체가 `#ifdef`로 통째로 빠져 있어서(`cmd/sunxi_sprite.c:26,35,42,46,55,59,69,73`), **바이너리에 그 코드가 존재하지 않는다** — 런타임 바이너리 패치(aw-tool이 지금 하는 work_mode 4바이트 패치 같은 것)로 되살릴 수 없다. 코드 자체를 다시 컴파일해 넣어야 한다.
- GPIO 핀/깜빡임 주기는 DTS → `.dtb`로 컴파일되어 boot0/u-boot 이미지에 임베드된다. 이 부분은 "값 몇 개"라 이론적으로 dtb 안의 해당 property를 바이너리 레벨에서 패치할 수도 있지만, `CONFIG_SUNXI_SPRITE_LED`가 꺼진 빌드에서는 그래도 코드가 없어 무의미하다. 따라서 **이것도 재빌드 플로우에 포함시키는 게 합리적**(dtb만 따로 패치하는 것보다 간단).
- T507 현재 defconfig(`configs/sun50iw9p1_t507_defconfig`)는 `CONFIG_SUNXI_SPRITE_LED`도 없고 `sprite_gpio0`도 주석 처리라 핀도 비어있다 — T507은 Kconfig + DTS 둘 다 손대야 한다.
- T527 현재 defconfig(`configs/sun55iw3p1_t527_defconfig`)도 `CONFIG_SUNXI_SPRITE_LED`가 꺼져 있지만, SoC 레퍼런스 DTS에는 핀이 이미 들어있어 **Kconfig 한 줄만 켜면 일단 동작할 가능성**이 있다(PC7이 pluto 보드에서 비어 있다는 전제 하에, 실기 확인 필요).

### 6.6 다음 단계 (LED/GPIO 관련)

1. u-boot 재빌드 툴체인/환경이 지금 갖춰져 있는지 확인 (`Makefile`, `scripts/`, 크로스컴파일러 — SDK에 `u-boot.bin` 등 산출물이 이미 있다는 건 최소 한 번은 빌드됐다는 뜻이나, 현재 호스트에서 재빌드 가능한지는 별도 확인 필요).
2. pluto 보드(T527 "pluto_wallpad"/"pluto_lobby", T507)의 실제 보드 DTS/스키매틱에서 LED용으로 쓸 수 있는 여분 GPIO 확정.
3. `configs/sun55iw3p1_t527_defconfig`(및 T507용 defconfig)에 `CONFIG_SUNXI_SPRITE_LED=y` 추가.
4. 보드 DTS의 `card_boot` 노드에 `sprite_gpio0`(확정된 핀), 필요하면 `sprite_work_delay`/`sprite_err_delay` 추가.
5. u-boot 재빌드 → `boot0_sdcard.fex`, `boot_package.fex`(TOC1) 재생성.
6. **재빌드된 boot0/uboot을 다시 IMAGEWTY 패키지에 넣는 절차 확인** — SDK 안에 패키징 스크립트가 있는지 미확인(이번 조사에서 `tools_win`에는 없었고, `lichee/tools/pack` 류 경로는 이 SDK 트리 구조에서 아직 못 찾음 — 다음 조사 대상). 안드로이드 쪽 `build.sh`/`pack.sh`가 이 역할을 할 가능성이 높다.
7. (3절 레이아웃과 합쳐) 재빌드된 boot0/TOC1 + 기존 firmware.img를 LBA 16/32800(24576)/40960에 쓰는 `aw-tool flash-sdcard`를 구현하고, 실기로 "LED 느리게 깜빡임 → 꺼짐(성공)" / "LED 느리게 깜빡임 → 빨리 깜빡임(실패, 예: 펌웨어를 일부러 깨뜨려서 테스트)"을 둘 다 검증.

---

## 7. FEL/EFEX 플래싱과의 차이점 정리

| | FEL/EFEX (현재 구현) | 카드 퓨징 (신규) |
|---|---|---|
| 연결 | USB (호스트 ↔ 보드) | 없음 — SD카드만 |
| 누가 파싱/write 하나 | 호스트(aw-tool, Rust) | **보드 자신의 u-boot**(`sunxi_card_sprite_main`) |
| work_mode 패치값 | `0x10`(USB_PRODUCT) | `0x11`(CARD_PRODUCT) |
| 프로토콜 | EFEX(CBW/CSW, `fes_down`/`fes_trans`) | 없음 — 고정 LBA에 raw write만 |
| 이미지 포맷 | IMAGEWTY (`firmware.img`) | **동일한 IMAGEWTY 파일**을 그대로 특정 LBA부터 복사 |
| 호스트 작업 | FEL 부트스트랩 + EFEX 세션 유지 + 섹터별 전송 | boot0/uboot/firmware.img를 정해진 LBA 3곳에 쓰기만 하면 끝 |
| 진행률 표시 | aw-tool이 호스트에서 EFEX 응답으로 추정 | 보드의 `sprite_led_*` (화면 없으면 LED) — **호스트는 퓨징 중엔 진행을 알 방법이 없음**(카드 꽂고 떼면 그만이라 보드 쪽 피드백이 유일한 방법) |
| aw-tool 구현 난이도 | 높음(USB/FEL/EFEX 프로토콜 전부 구현) | **낮음** — 기존 `imagewty.rs`/`sunxi_head.rs`를 재사용해 raw block device에 3번 write |

---

## 8. `aw-tool flash-sdcard` 구현에 필요한 다음 단계 / 미확인 사항

**구현 자체는 비교적 단순**하다 — 이미 있는 코드를 재사용:

1. 기존 `firmware.img`에서 boot0 아이템을 꺼내 `set_work_mode(..., WORK_MODE_CARD_PRODUCT)` + `fix_checksum()` (이미 `src/sunxi_head.rs`에 함수 존재, FEL 경로는 `WORK_MODE_USB_PRODUCT`로 호출하던 걸 다른 상수로 호출만 바꾸면 됨).
2. boot_package(TOC1)는 **패치 없이 그대로** 꺼낸다.
3. 대상이 raw block device(`/dev/sdX`, macOS `/dev/rdiskN`, Windows는 `\\.\PhysicalDriveN` 류)인지 확인 — **잘못된 디바이스를 고르면 사용자 PC의 디스크를 덮어쓸 수 있어 안전장치(용량 확인, 분리형 미디어 여부 확인, 확인 프롬프트)가 필수**.
4. LBA16/32800/24576/40960에 각각 write.

**미확인·다음 조사 대상**:

- `sunxi_card_sprite_main()`의 세부 흐름(인자 `workmode, name`이 어떻게 쓰이는지, 보드가 카드의 "어느 파티션 몇 번"을 읽는지의 정확한 디스커버리 로직) — 2절은 `sprite_card.c` 앞부분(1~1472행)만 읽었고 뒷부분(1473~1908행)과 `sprite_main.c`의 헤더 탐지(카드 맨 앞에서 "이게 카드 퓨징 카드다"를 인식하는 매직/체크)는 아직 못 봤다.
- 24,576~32,800, 32,800+uboot크기~40,960 사이 "예비 영역"을 0으로 채워야 하는지 완전히 비워도 되는지 — 매뉴얼은 "원래 데이터가 남아있어도 무방"이라 썼지만 소스로 재확인 필요.
- MBR/GPT를 카드에 전혀 안 만들어도 되는지 — `sprite_card_fetch_mbr()`가 IMAGEWTY 안의 MBR을 읽어서 **내장 flash용 MBR**을 만드는 거라, 카드 자체의 파티션 테이블과는 무관해 보이나 100% 확신하려면 `sprite_main.c`를 더 봐야 한다.
- pluto 보드 실물의 LED 핀 확정(6.4절) — 조사 범위 밖, 보드 자료 필요.
- u-boot 재패키징 스크립트 위치(6.6절 6번) — android 쪽 빌드 스크립트 확인 필요.
- 실기 테스트 전무 — 이 문서의 모든 레이아웃/오프셋은 **소스+매뉴얼 기반 추정이며 실기로 아직 검증되지 않았다**. FEL/EFEX 조사 때도 실기에서 로드 주소 오류로 두 차례 보드가 응답 불능이 됐던 선례가 있으므로, 처음 실기 테스트는 복구 가능한(이미 FEL로 복구 경험이 있는) 보드로 하고, 카드 퓨징이 실패해도 보드가 벽돌이 되지는 않는지(= 실패해도 그냥 "WORK_MODE_CARD_PRODUCT로 멈춰서 LED만 깜빡이는" 상태인지) 먼저 확인하는 게 안전하다.
