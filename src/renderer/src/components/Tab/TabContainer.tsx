import { isLinux, isMac } from '@renderer/config/constant'
import { useFullscreen } from '@renderer/hooks/useFullscreen'
import { useSettings } from '@renderer/hooks/useSettings'
import UpdateAppButton from '@renderer/pages/home/components/UpdateAppButton'
import type { FC } from 'react'
import styled from 'styled-components'

import MinAppTabsPool from '../MinApp/MinAppTabsPool'
import WindowControls from '../WindowControls'

interface TabsContainerProps {
  children: React.ReactNode
}

const TabsContainer: FC<TabsContainerProps> = ({ children }) => {
  const isFullscreen = useFullscreen()
  const { useSystemTitleBar } = useSettings()

  return (
    <Container>
      <TabsBar $isFullscreen={isFullscreen}>
        <RightButtonsContainer style={{ paddingRight: isLinux && useSystemTitleBar ? '12px' : undefined }}>
          <UpdateAppButton />
        </RightButtonsContainer>
        <WindowControls />
      </TabsBar>
      <TabContent>
        {/* MiniApp WebView 池（Tab 模式保活） */}
        <MinAppTabsPool />
        {children}
      </TabContent>
    </Container>
  )
}

const Container = styled.div`
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;
  min-width: 0;
`

const TabsBar = styled.div<{ $isFullscreen: boolean }>`
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 5px;
  padding-left: ${({ $isFullscreen }) => (!$isFullscreen && isMac ? 'calc(env(titlebar-area-x) + 4px)' : '15px')};
  padding-right: ${({ $isFullscreen }) => ($isFullscreen ? '12px' : '0')};
  height: var(--navbar-height);
  min-height: ${({ $isFullscreen }) => (!$isFullscreen && isMac ? 'env(titlebar-area-height)' : '')};
  position: relative;
  -webkit-app-region: drag;

  /* 确保交互元素在拖拽区域之上 */
  > * {
    position: relative;
    z-index: 1;
    -webkit-app-region: no-drag;
  }
`

const RightButtonsContainer = styled.div`
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  padding-right: ${isMac ? '12px' : '0'};
  flex-shrink: 0;
`

const TabContent = styled.div`
  display: flex;
  flex: 1;
  overflow: hidden;
  min-width: 0;
  margin: 6px;
  margin-top: 0;
  border-radius: 8px;
  overflow: hidden;
  position: relative; /* 约束 MinAppTabsPool 绝对定位范围 */
`

export default TabsContainer
