package moe.fuqiuluo.mamu.floating.controller

import android.annotation.SuppressLint
import android.content.Context
import android.view.View
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import kotlinx.coroutines.*
import moe.fuqiuluo.mamu.R
import moe.fuqiuluo.mamu.floating.data.model.SavedAddress
import moe.fuqiuluo.mamu.databinding.FloatingSavedAddressesLayoutBinding
import moe.fuqiuluo.mamu.floating.adapter.SavedAddressAdapter
import moe.fuqiuluo.mamu.floating.data.model.DisplayProcessInfo
import moe.fuqiuluo.mamu.utils.ByteFormatUtils.formatBytes
import moe.fuqiuluo.mamu.widget.NotificationOverlay

class SavedAddressController(
    context: Context,
    binding: FloatingSavedAddressesLayoutBinding,
    notification: NotificationOverlay
) : FloatingController<FloatingSavedAddressesLayoutBinding>(context, binding, notification) {
    // 保存的地址列表（内存中）
    private val savedAddresses = mutableListOf<SavedAddress>()

    // 地址数量
    private var addressCountBadgeView: TextView? = null

    // 列表适配器
    private val adapter: SavedAddressAdapter = SavedAddressAdapter(
        onItemClick = { address, position ->
            notification.showWarning("点击了 ${address.name}")
        },
        onFreezeToggle = { address, isFrozen ->
            // 切换冻结状态
            val index = savedAddresses.indexOfFirst { it.address == address.address }
            if (index >= 0) {
                savedAddresses[index] = savedAddresses[index].copy(isFrozen = isFrozen)
                notification.showSuccess(if (isFrozen) "已冻结" else "已解除冻结")
            }
        },
        onItemDelete = { address ->
            deleteAddress(address.address)
        }
    )

    private val coroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    override fun initialize() {
        setupToolbar()
        setupRecyclerView()
        setupRefreshButton()
        updateProcessDisplay(null)
        updateEmptyState()
        updateAddressCountBadge()
    }

    fun setAddressCountBadgeView(badge: TextView) {
        addressCountBadgeView = badge
        updateAddressCountBadge()
    }

    private fun updateAddressCountBadge() {
        addressCountBadgeView?.let { badge ->
            val count = savedAddresses.size
            if (count > 0) {
                badge.text = if (count > 99) "99+" else count.toString()
                badge.visibility = View.VISIBLE
            } else {
                badge.visibility = View.GONE
            }
        }
    }

    private fun setupToolbar() {
        // TODO: 添加工具栏操作
    }

    private fun setupRecyclerView() {
        binding.savedAddressesRecyclerView.apply {
            layoutManager = LinearLayoutManager(context)
            adapter = this@SavedAddressController.adapter
            setHasFixedSize(true)
        }
    }

    private fun setupRefreshButton() {
        binding.refreshButton.setOnClickListener {
            refreshAddresses()
        }
    }

    @SuppressLint("SetTextI18n")
    fun updateProcessDisplay(process: DisplayProcessInfo?) {
        process?.let {
            val memoryB = (it.rss * 4096)
            binding.processInfoText.text =
                "[${it.pid}] ${it.name} [${formatBytes(memoryB, 0)}]"
            binding.processStatusIcon.setIconResource(R.drawable.icon_pause_24px)
        } ?: run {
            binding.processInfoText.text = "未选择进程"
            binding.processStatusIcon.setIconResource(R.drawable.icon_play_arrow_24px)
        }
    }

    /**
     * 保存单个地址
     */
    fun saveAddress(address: SavedAddress) {
        val existingIndex = savedAddresses.indexOfFirst { it.address == address.address }
        if (existingIndex >= 0) {
            savedAddresses[existingIndex] = address
            adapter.updateAddress(address)
        } else {
            savedAddresses.add(address)
            adapter.addAddress(address)
        }
        updateEmptyState()
        updateAddressCountBadge()
    }

    /**
     * 批量保存地址
     */
    fun saveAddresses(addresses: List<SavedAddress>) {
        if (addresses.isEmpty()) {
            return
        }

        addresses.forEach { newAddr ->
            val existingIndex = savedAddresses.indexOfFirst { it.address == newAddr.address }
            if (existingIndex >= 0) {
                savedAddresses[existingIndex] = newAddr
            } else {
                savedAddresses.add(newAddr)
            }
        }
        adapter.setAddresses(savedAddresses)
        updateEmptyState()
        updateAddressCountBadge()

        notification.showSuccess("已保存 ${addresses.size} 个地址")
    }

    /**
     * 删除地址
     */
    private fun deleteAddress(address: Long) {
        savedAddresses.removeIf { it.address == address }
        adapter.setAddresses(savedAddresses)
        updateEmptyState()
        updateAddressCountBadge()
        notification.showSuccess("已删除")
    }

    /**
     * 清空所有地址（进程切换或死亡时调用）
     */
    fun clearAll() {
        savedAddresses.clear()
        adapter.setAddresses(emptyList())
        updateEmptyState()
        updateAddressCountBadge()
    }

    /**
     * 刷新所有地址的值
     */
    private fun refreshAddresses() {
        if (savedAddresses.isEmpty()) {
            notification.showWarning("没有保存的地址")
            return
        }

        // TODO: 从内存读取最新值并更新
        notification.showWarning("刷新功能待实现")
    }

    private fun updateEmptyState() {
        if (savedAddresses.isEmpty()) {
            binding.emptyStateView.visibility = View.VISIBLE
            binding.savedAddressesRecyclerView.visibility = View.GONE
        } else {
            binding.emptyStateView.visibility = View.GONE
            binding.savedAddressesRecyclerView.visibility = View.VISIBLE
        }
    }

    override fun cleanup() {
        super.cleanup()
        coroutineScope.cancel()
    }
}