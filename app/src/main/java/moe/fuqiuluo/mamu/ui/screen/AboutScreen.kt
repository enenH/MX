package moe.fuqiuluo.mamu.ui.screen

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.material3.windowsizeclass.WindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import moe.fuqiuluo.mamu.data.model.Acknowledgment
import moe.fuqiuluo.mamu.data.model.LibraryCategory
import moe.fuqiuluo.mamu.data.model.OpenSourceLibrary
import moe.fuqiuluo.mamu.ui.theme.AdaptiveLayoutInfo
import moe.fuqiuluo.mamu.ui.theme.Dimens
import moe.fuqiuluo.mamu.ui.theme.rememberAdaptiveLayoutInfo

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(
    windowSizeClass: WindowSizeClass,
    onNavigateBack: () -> Unit
) {
    val adaptiveLayout = rememberAdaptiveLayoutInfo(windowSizeClass)
    BackHandler(onBack = onNavigateBack)

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("关于 Mamu") },
                navigationIcon = {
                    IconButton(onClick = onNavigateBack) {
                        Icon(
                            imageVector = Icons.Default.ArrowBack,
                            contentDescription = "返回"
                        )
                    }
                }
            )
        }
    ) { paddingValues ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Column(
                modifier = Modifier
                    .widthIn(
                        max = when (adaptiveLayout.windowSizeClass.widthSizeClass) {
                            WindowWidthSizeClass.Compact -> adaptiveLayout.contentMaxWidth
                            else -> 720.dp // 横屏时使用更宽的最大宽度
                        }
                    )
                    .fillMaxWidth()
                    .padding(paddingValues)
                    .padding(Dimens.paddingLg(adaptiveLayout))
            ) {
                // 项目信息卡片
                ProjectInfoCard(adaptiveLayout)
                Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

                // 开源依赖分类
                getLibraryCategories().forEach { category ->
                    LibraryCategoryCard(
                        adaptiveLayout = adaptiveLayout,
                        category = category
                    )
                    Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))
                }

                // 特别感谢
                AcknowledgmentCard(
                    adaptiveLayout = adaptiveLayout,
                    acknowledgments = getAcknowledgments()
                )
                Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

                // 版本信息
                VersionInfoCard(adaptiveLayout)

                // 底部间距
                Spacer(modifier = Modifier.height(Dimens.spacingLg(adaptiveLayout)))
            }
        }
    }
}

@Composable
private fun ProjectInfoCard(adaptiveLayout: AdaptiveLayoutInfo) {
    val context = LocalContext.current

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier.padding(Dimens.paddingLg(adaptiveLayout)),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            // 应用图标
            Icon(
                imageVector = Icons.Default.Memory,
                contentDescription = null,
                modifier = Modifier.size(64.dp),
                tint = MaterialTheme.colorScheme.primary
            )

            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

            Text(
                text = "Mamu",
                style = MaterialTheme.typography.headlineMedium,
                fontWeight = FontWeight.Bold
            )

            Text(
                text = "版本 1.0.0",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

            Text(
                text = "基于 Root 权限的 Android 内存调试工具",
                style = MaterialTheme.typography.bodySmall,
                textAlign = TextAlign.Center,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )

            Spacer(modifier = Modifier.height(Dimens.spacingLg(adaptiveLayout)))

            // GitHub 链接按钮
            OutlinedButton(
                onClick = {
                    openUrl(context, "https://github.com/Shirasuki/MX")
                }
            ) {
                Icon(
                    imageVector = Icons.Default.OpenInNew,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSm(adaptiveLayout))
                )
                Spacer(modifier = Modifier.width(Dimens.spacingSm(adaptiveLayout)))
                Text("GitHub 仓库")
            }

            Spacer(modifier = Modifier.height(Dimens.spacingSm(adaptiveLayout)))

            Text(
                text = "基于 android-wuwa 项目开发",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.secondary
            )

            // 许可证信息
            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))
            HorizontalDivider()
            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    imageVector = Icons.Default.Gavel,
                    contentDescription = null,
                    modifier = Modifier.size(Dimens.iconSm(adaptiveLayout)),
                    tint = MaterialTheme.colorScheme.tertiary
                )
                Spacer(modifier = Modifier.width(Dimens.spacingSm(adaptiveLayout)))
                Text(
                    text = "GNU GPL v3.0 License",
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.tertiary
                )
            }
        }
    }
}

@Composable
private fun LibraryCategoryCard(
    adaptiveLayout: AdaptiveLayoutInfo,
    category: LibraryCategory
) {
    var expanded by remember { mutableStateOf(false) }

    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { expanded = !expanded }
    ) {
        Column(modifier = Modifier.padding(Dimens.paddingLg(adaptiveLayout))) {
            // 标题行
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = category.categoryName,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold
                    )
                    Text(
                        text = "${category.libraries.size} 个依赖",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }

                Icon(
                    imageVector = if (expanded) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                    contentDescription = if (expanded) "收起" else "展开",
                    tint = MaterialTheme.colorScheme.primary
                )
            }

            // 可折叠内容
            AnimatedVisibility(visible = expanded) {
                Column(modifier = Modifier.padding(top = Dimens.spacingMd(adaptiveLayout))) {
                    category.libraries.forEach { library ->
                        LibraryItem(
                            adaptiveLayout = adaptiveLayout,
                            library = library
                        )
                        if (library != category.libraries.last()) {
                            HorizontalDivider(
                                modifier = Modifier.padding(
                                    vertical = Dimens.spacingSm(adaptiveLayout)
                                )
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LibraryItem(
    adaptiveLayout: AdaptiveLayoutInfo,
    library: OpenSourceLibrary
) {
    val context = LocalContext.current

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { openUrl(context, library.url) }
            .padding(vertical = Dimens.spacingXs(adaptiveLayout))
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                text = library.name,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.weight(1f)
            )

            AssistChip(
                onClick = { openUrl(context, library.url) },
                label = {
                    Text(
                        text = library.license,
                        style = MaterialTheme.typography.labelSmall
                    )
                },
                leadingIcon = {
                    Icon(
                        imageVector = Icons.Default.OpenInNew,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp)
                    )
                }
            )
        }

        Spacer(modifier = Modifier.height(Dimens.spacingXs(adaptiveLayout)))

        Text(
            text = library.description,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@Composable
private fun AcknowledgmentCard(
    adaptiveLayout: AdaptiveLayoutInfo,
    acknowledgments: List<Acknowledgment>
) {
    val context = LocalContext.current

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(Dimens.paddingLg(adaptiveLayout))) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Dimens.spacingSm(adaptiveLayout))
            ) {
                Icon(
                    imageVector = Icons.Default.Favorite,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error
                )
                Text(
                    text = "特别感谢",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold
                )
            }

            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

            acknowledgments.forEach { ack ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(enabled = ack.url != null) {
                            ack.url?.let { openUrl(context, it) }
                        }
                        .padding(vertical = Dimens.spacingSm(adaptiveLayout)),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = ack.name,
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium
                        )
                        Text(
                            text = ack.description,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }

                    if (ack.url != null) {
                        Icon(
                            imageVector = Icons.Default.OpenInNew,
                            contentDescription = null,
                            modifier = Modifier.size(Dimens.iconSm(adaptiveLayout)),
                            tint = MaterialTheme.colorScheme.primary
                        )
                    }
                }

                if (ack != acknowledgments.last()) {
                    HorizontalDivider(
                        modifier = Modifier.padding(
                            vertical = Dimens.spacingSm(adaptiveLayout)
                        )
                    )
                }
            }
        }
    }
}

@Composable
private fun VersionInfoCard(adaptiveLayout: AdaptiveLayoutInfo) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(Dimens.paddingLg(adaptiveLayout))) {
            Text(
                text = "版本信息",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold
            )

            Spacer(modifier = Modifier.height(Dimens.spacingMd(adaptiveLayout)))

            InfoRow(
                adaptiveLayout = adaptiveLayout,
                label = "应用版本",
                value = "1.0.0 (1)"
            )

            InfoRow(
                adaptiveLayout = adaptiveLayout,
                label = "构建类型",
                value = "Debug"
            )

            InfoRow(
                adaptiveLayout = adaptiveLayout,
                label = "目标架构",
                value = "ARM64-v8a"
            )
        }
    }
}

@Composable
private fun InfoRow(
    adaptiveLayout: AdaptiveLayoutInfo,
    label: String,
    value: String
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = Dimens.spacingXs(adaptiveLayout)),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium
        )
    }
}

/**
 * 打开 URL 的工具函数
 */
private fun openUrl(context: Context, url: String) {
    try {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
        context.startActivity(intent)
    } catch (e: Exception) {
        Toast.makeText(context, "无法打开链接", Toast.LENGTH_SHORT).show()
    }
}

/**
 * 获取开源库分类列表
 */
private fun getLibraryCategories(): List<LibraryCategory> {
    return listOf(
        LibraryCategory("UI 框架", listOf(
            OpenSourceLibrary("Jetpack Compose", "Android 现代声明式 UI", "Apache 2.0", "https://developer.android.com/jetpack/compose"),
            OpenSourceLibrary("Material Design 3", "Google Material Design 组件库", "Apache 2.0", "https://m3.material.io/"),
            OpenSourceLibrary("Material Icons Extended", "Material Design 图标库", "Apache 2.0", "https://developer.android.com/")
        )),
        LibraryCategory("Android 核心", listOf(
            OpenSourceLibrary("AndroidX Core KTX", "Android 核心扩展库", "Apache 2.0", "https://developer.android.com/kotlin/ktx"),
            OpenSourceLibrary("AndroidX Lifecycle", "生命周期感知组件", "Apache 2.0", "https://developer.android.com/"),
            OpenSourceLibrary("AndroidX Activity Compose", "Activity Compose 集成", "Apache 2.0", "https://developer.android.com/")
        )),
        LibraryCategory("数据存储", listOf(
            OpenSourceLibrary("MMKV", "腾讯高性能键值存储", "BSD", "https://github.com/Tencent/MMKV")
        )),
        LibraryCategory("异步与并发", listOf(
            OpenSourceLibrary("Kotlin Coroutines", "Kotlin 协程库", "Apache 2.0", "https://kotlinlang.org/docs/coroutines-overview.html")
        )),
        LibraryCategory("Root 管理", listOf(
            OpenSourceLibrary("libsu", "topjohnwu 的 Root Shell 库", "Apache 2.0", "https://github.com/topjohnwu/libsu")
        )),
        LibraryCategory("传统 View 组件", listOf(
            OpenSourceLibrary("AppCompat", "Android 兼容库", "Apache 2.0", "https://developer.android.com/"),
            OpenSourceLibrary("RecyclerView", "高效列表组件", "Apache 2.0", "https://developer.android.com/"),
            OpenSourceLibrary("ViewPager2", "页面滑动组件", "Apache 2.0", "https://developer.android.com/"),
            OpenSourceLibrary("ConstraintLayout", "约束布局", "Apache 2.0", "https://developer.android.com/"),
            OpenSourceLibrary("CardView", "卡片视图", "Apache 2.0", "https://developer.android.com/")
        )),
        LibraryCategory("工具库", listOf(
            OpenSourceLibrary("kotlin-csv", "CSV 文件处理", "Apache 2.0", "https://github.com/jsoizo/kotlin-csv"),
            OpenSourceLibrary("fastutil", "高性能集合库", "Apache 2.0", "https://fastutil.di.unimi.it/"),
            OpenSourceLibrary("kotlinx-io", "Kotlin IO 库", "Apache 2.0", "https://github.com/Kotlin/kotlinx-io")
        )),
        LibraryCategory("Rust 核心运行时", listOf(
            OpenSourceLibrary("tokio", "异步运行时", "MIT", "https://tokio.rs/"),
            OpenSourceLibrary("nix", "Unix 系统调用", "MIT", "https://github.com/nix-rust/nix"),
            OpenSourceLibrary("jni", "Java Native Interface", "Apache 2.0/MIT", "https://github.com/jni-rs/jni-rs")
        )),
        LibraryCategory("Rust 数据并行", listOf(
            OpenSourceLibrary("rayon", "数据并行库", "Apache 2.0/MIT", "https://github.com/rayon-rs/rayon")
        )),
        LibraryCategory("Rust 序列化与网络", listOf(
            OpenSourceLibrary("serde", "序列化框架", "Apache 2.0/MIT", "https://serde.rs/"),
            OpenSourceLibrary("reqwest", "HTTP 客户端", "Apache 2.0/MIT", "https://github.com/seanmonstar/reqwest"),
            OpenSourceLibrary("rustls", "TLS 实现", "Apache 2.0/MIT", "https://github.com/rustls/rustls")
        )),
        LibraryCategory("Rust 内存操作", listOf(
            OpenSourceLibrary("memmap2", "内存映射", "Apache 2.0/MIT", "https://github.com/RazrFalcon/memmap2-rs"),
            OpenSourceLibrary("memchr", "内存搜索", "MIT", "https://github.com/BurntSushi/memchr"),
            OpenSourceLibrary("bytemuck", "类型安全转换", "Zlib", "https://github.com/Lokathor/bytemuck")
        )),
        LibraryCategory("Rust 其他工具", listOf(
            OpenSourceLibrary("anyhow", "错误处理", "Apache 2.0/MIT", "https://github.com/dtolnay/anyhow"),
            OpenSourceLibrary("log", "日志门面", "Apache 2.0/MIT", "https://github.com/rust-lang/log"),
            OpenSourceLibrary("android_logger", "Android 日志", "Apache 2.0/MIT", "https://github.com/Nercury/android_logger-rs"),
            OpenSourceLibrary("obfstr", "字符串混淆", "MIT", "https://github.com/CasualX/obfstr"),
            OpenSourceLibrary("capstone", "反汇编引擎", "BSD", "https://github.com/capstone-engine/capstone"),
            OpenSourceLibrary("lazy_static", "延迟静态变量", "Apache 2.0/MIT", "https://github.com/rust-lang-nursery/lazy-static.rs"),
            OpenSourceLibrary("rand", "随机数生成", "Apache 2.0/MIT", "https://github.com/rust-random/rand"),
            OpenSourceLibrary("zip", "ZIP 压缩", "MIT", "https://github.com/zip-rs/zip")
        ))
    )
}

/**
 * 获取特别感谢列表
 */
private fun getAcknowledgments(): List<Acknowledgment> {
    return listOf(
        Acknowledgment("GameGuardian", "Android 内存修改工具的灵感来源", "https://gameguardian.net/"),
        Acknowledgment("Cheat Engine", "PC 端内存扫描器与调试器", "https://www.cheatengine.org/"),
        Acknowledgment("Magisk / KernelSU", "Root 访问基础设施", "https://github.com/topjohnwu/Magisk"),
        Acknowledgment("niqiuqiux's PointerScan", "C++ 指针链扫描实现", "https://github.com/niqiuqiux/PointerScan"),
        Acknowledgment("android-wuwa", "本项目的基础", "https://github.com/fuqiuluo/android-wuwa")
    )
}
